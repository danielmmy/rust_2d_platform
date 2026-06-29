//! Main menu (title screen) and pause menu.
//!
//! The game boots into [`GameState::MainMenu`]. The title screen has a few screens:
//! the main list, a save-slot picker for **New Game** / **Load Game** (see
//! [`crate::save`]), and an **Options** screen (window mode). During play, `Esc` (or the
//! gamepad `Select` button) toggles a [`Paused`] overlay (Continue / **Character** /
//! **Abilities** / **Controls** / Options / Main Menu / Quit); **Character** opens a read-only
//! stat sheet, **Abilities** toggles unlocked abilities on/off, **Controls** shows the
//! keyboard + controller layout reference, and **Options** the window-mode/volume settings.
//! Gameplay is frozen while paused (the gameplay [`GameSet`](crate::GameSet) chain
//! is gated on [`Paused::Running`]).
//!
//! The window-mode choice (windowed / borderless fullscreen) persists via
//! [`crate::save::Settings`] and is applied to the primary window by `apply_window_mode`.
//!
//! Menus are drawn the lightweight way the world map is — a backdrop sprite plus
//! `Text2d` rows around the camera — and navigated with the arrows / D-pad, confirmed
//! with jump / `Enter` / `South`, and stepped **back / cancelled** with `Esc` / **Circle**
//! (in the pause menu, **Select** also closes it outright). **Quit** asks for confirmation
//! first so a misclick can't drop you out of the game.

use bevy::app::AppExit;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::combat::{Energy, LostEnergy};
use crate::player::{Abilities, Ability};
use crate::save::{self, GameMode, SLOTS, Save, Settings};
use crate::state::GameState;
use crate::stats::{Stats, character_lines};
use crate::world::{
    LevelRoot, PendingSpawn, START_MAP, SpawnRequest, builder_maps_dir, init_builder_world,
};
use crate::worldmap::MapView;

/// Whether gameplay is paused. Gameplay runs only when `Running`.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Paused {
    #[default]
    Running,
    Paused,
}

/// Which title-screen list is showing.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum MenuScreen {
    #[default]
    Main,
    /// Pick a slot for a fresh game.
    NewSlots,
    /// Pick a slot to load.
    LoadSlots,
    /// Confirm overwriting an occupied slot with a new game (carries the slot).
    ConfirmNew(usize),
    /// Choose Story or Builder for the new game in this slot.
    ModeSelect(usize),
    /// Type a name for the new game in this slot/mode, then start.
    NameEntry(usize, GameMode),
    /// Settings (window mode).
    Options,
    /// Confirm quitting the whole game (guards against a misclick).
    ConfirmQuit,
}

/// Which pause sub-screen is showing.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum PauseScreen {
    #[default]
    Root,
    /// The read-only character status sheet.
    Character,
    /// Settings (window mode).
    Options,
    /// Toggle abilities on/off (all of them in a Builder save; only acquired in Story).
    Abilities,
    /// A read-only reference of the keyboard + controller layouts.
    Controls,
    /// Confirm quitting the whole game (guards against a misclick).
    ConfirmQuit,
}

/// The name being typed for a new game (in the [`MenuScreen::NameEntry`] screen).
#[derive(Resource, Default)]
struct NewGameName(String);

/// Max length of a save name.
const NAME_MAX: usize = 20;

/// Read-only resources for the character sheet, bundled so the pause system stays under
/// Bevy's parameter limit.
#[derive(SystemParam)]
struct CharInfo<'w> {
    stats: Res<'w, Stats>,
    energy: Res<'w, Energy>,
    lost: Res<'w, LostEnergy>,
}

/// Tags every entity that makes up a menu (despawned when it closes or redraws).
#[derive(Component)]
struct MenuEntity;

/// One selectable menu row, by index from the top.
#[derive(Component)]
struct MenuItem(usize);

/// The highlighted row (reset when a menu opens or its screen changes).
#[derive(Resource, Default)]
struct MenuCursor(usize);

/// What a menu row does when chosen.
#[derive(Clone, Copy)]
enum MenuAction {
    NewGame,
    LoadGame,
    PickSlot(usize),
    /// Confirm erasing the occupied slot (held in the screen) and start fresh.
    ConfirmNew,
    /// Choose the new game's mode (then name it).
    PickMode(GameMode),
    Back,
    Continue,
    /// Open the pause menu's read-only character status sheet.
    Character,
    /// Open the level builder (Builder saves only).
    OpenEditor,
    /// Open the settings screen.
    OpenOptions,
    /// Open the abilities sub-screen.
    OpenAbilities,
    /// Open the read-only controls reference.
    OpenControls,
    /// Toggle an ability on/off (grants/removes in a Builder save; flips active in Story).
    ToggleAbility(Ability),
    /// Set the window mode (true = borderless fullscreen).
    SetFullscreen(bool),
    /// Cycle the sound-effects volume up one step (wrapping).
    StepFxVolume,
    /// Cycle the music volume up one step (wrapping).
    StepMusicVolume,
    /// Leave a paused game back to the title screen.
    MainMenu,
    /// Open the quit-confirmation screen (guards against a misclick).
    Quit,
    /// Actually exit the app — only chosen from the quit-confirmation screen.
    QuitConfirmed,
}

/// A 5-segment ASCII level bar + percent for a 0..1 volume.
fn volume_bar(v: f32) -> String {
    let filled = (v * 5.0).round() as usize;
    let pct = (v * 100.0).round() as i32;
    format!("[{}{}] {pct}%", "#".repeat(filled), "-".repeat(5 - filled))
}

/// The next volume level (steps of 20%, wrapping 100% -> 0%); confirm cycles a row.
fn step_volume(v: f32) -> f32 {
    let level = (v * 5.0).round() as i32;
    ((level + 1).rem_euclid(6)) as f32 / 5.0
}

/// The settings rows: window mode plus FX / music volumes (confirm a volume to cycle it).
fn options_items(settings: &Settings) -> Vec<(String, MenuAction)> {
    let mark = |on: bool| if on { "[x]" } else { "[ ]" };
    vec![
        (
            format!("{} Windowed", mark(!settings.fullscreen)),
            MenuAction::SetFullscreen(false),
        ),
        (
            format!("{} Fullscreen (borderless)", mark(settings.fullscreen)),
            MenuAction::SetFullscreen(true),
        ),
        (
            format!("FX volume     {}", volume_bar(settings.fx_volume)),
            MenuAction::StepFxVolume,
        ),
        (
            format!("Music volume  {}", volume_bar(settings.music_volume)),
            MenuAction::StepMusicVolume,
        ),
        ("Back".to_string(), MenuAction::Back),
    ]
}

fn main_menu_items(screen: MenuScreen, settings: &Settings) -> Vec<(String, MenuAction)> {
    match screen {
        MenuScreen::Main => vec![
            ("New Game".to_string(), MenuAction::NewGame),
            ("Load Game".to_string(), MenuAction::LoadGame),
            ("Options".to_string(), MenuAction::OpenOptions),
            ("Quit".to_string(), MenuAction::Quit),
        ],
        MenuScreen::Options => options_items(settings),
        MenuScreen::NewSlots | MenuScreen::LoadSlots => {
            let mut items: Vec<(String, MenuAction)> = (0..SLOTS)
                .map(|i| (slot_label(i), MenuAction::PickSlot(i)))
                .collect();
            items.push(("Back".to_string(), MenuAction::Back));
            items
        }
        MenuScreen::ConfirmNew(slot) => vec![
            (
                format!("Erase slot {} and start", slot + 1),
                MenuAction::ConfirmNew,
            ),
            ("Back".to_string(), MenuAction::Back),
        ],
        MenuScreen::ModeSelect(_) => vec![
            (
                "Story  (play the shipped levels)".to_string(),
                MenuAction::PickMode(GameMode::Story),
            ),
            (
                "Builder  (start a copy you can edit)".to_string(),
                MenuAction::PickMode(GameMode::Builder),
            ),
            ("Back".to_string(), MenuAction::Back),
        ],
        MenuScreen::ConfirmQuit => vec![
            ("Quit game".to_string(), MenuAction::QuitConfirmed),
            ("Back".to_string(), MenuAction::Back),
        ],
        // Drawn specially (it needs the live name buffer) — see `draw_name_entry`.
        MenuScreen::NameEntry(..) => Vec::new(),
    }
}

/// A one-line summary of a save slot for the picker (with its mode tag).
fn slot_label(slot: usize) -> String {
    match save::read_slot(slot) {
        Some(s) => {
            let detail = if !s.name.is_empty() {
                s.name
            } else if s.has_bench() {
                format!("bench in {}", s.bench_room)
            } else {
                "new game".to_string()
            };
            format!("Slot {} - [{}] {}", slot + 1, s.mode.tag(), detail)
        }
        None => format!("Slot {} - empty", slot + 1),
    }
}

fn pause_menu_items(builder: bool) -> Vec<(String, MenuAction)> {
    let mut items = vec![
        ("Continue".to_string(), MenuAction::Continue),
        ("Character".to_string(), MenuAction::Character),
        ("Abilities".to_string(), MenuAction::OpenAbilities),
        ("Controls".to_string(), MenuAction::OpenControls),
    ];
    if builder {
        items.push(("Edit Levels".to_string(), MenuAction::OpenEditor));
    }
    items.push(("Options".to_string(), MenuAction::OpenOptions));
    items.push(("Main Menu".to_string(), MenuAction::MainMenu));
    items.push(("Quit".to_string(), MenuAction::Quit));
    items
}

/// One row of the controls sheet: a plain-text action label plus its keyboard and
/// controller tokens (the latter two render in the icon font).
struct ControlRow {
    label: &'static str,
    key: &'static str,
    pad: &'static str,
}

impl ControlRow {
    const fn new(label: &'static str, key: &'static str, pad: &'static str) -> Self {
        Self { label, key, pad }
    }
}

/// The controls grouped into titled sections. Unlockable abilities are gated: a Story save
/// shows each only once acquired (no spoilers); a Builder save lists them all.
fn control_sections(builder: bool, save: &Save) -> Vec<(&'static str, Vec<ControlRow>)> {
    use crate::glyph::*;
    let have = Abilities::from_csv(&save.abilities);

    // The combat/ability section grows with what the player has earned.
    let mut actions = vec![ControlRow::new("Attack", "J", SQUARE)];
    for (ability, row) in [
        (
            Ability::DoubleJump,
            ControlRow::new("Double jump  (in mid-air)", SPACE, CROSS),
        ),
        (
            Ability::WallJump,
            ControlRow::new("Wall jump  (off a wall)", SPACE, CROSS),
        ),
        (
            Ability::Dash,
            ControlRow::new("Dash  (hold to run)", SHIFT, R1),
        ),
        (
            // Down + attack, on the ground or in the air. The pad token is a literal combo
            // (d-pad down + Square) so it can't reuse the `glyph` consts; escapes keep it ASCII.
            Ability::Pogo,
            ControlRow::new("Pogo  (down-slash)", "S J", "\u{21A1} \u{21E0}"),
        ),
    ] {
        if builder || have.has(ability) {
            actions.push(row);
        }
    }

    vec![
        (
            "MOVEMENT",
            vec![
                ControlRow::new("Move", "A D", STICK),
                ControlRow::new("Look up / crouch", "W S", DPAD_UD),
            ],
        ),
        ("ACTIONS", actions),
        (
            "MENU",
            vec![
                ControlRow::new("Interact / rest", "E", TRIANGLE),
                ControlRow::new("Character screen", "C", L1),
                ControlRow::new("World map", "M", OPTIONS),
                ControlRow::new("Pause", ESC, SHARE),
            ],
        ),
    ]
}

/// The ability sub-screen's rows: all abilities in a Builder save (grant/remove), or just the
/// acquired ones in a Story save (toggle active). Each shows `[x]`/`[ ]` from the live state.
fn ability_items(builder: bool, abilities: &Abilities, save: &Save) -> Vec<(String, MenuAction)> {
    let rows: Vec<Ability> = if builder {
        Ability::ALL.to_vec()
    } else {
        let have = Abilities::from_csv(&save.abilities);
        Ability::ALL.into_iter().filter(|a| have.has(*a)).collect()
    };
    let mut items: Vec<(String, MenuAction)> = rows
        .into_iter()
        .map(|a| {
            let mark = if abilities.has(a) { "[x]" } else { "[ ]" };
            (
                format!("{mark} {}", a.label()),
                MenuAction::ToggleAbility(a),
            )
        })
        .collect();
    if items.is_empty() {
        items.push(("(none acquired yet)".to_string(), MenuAction::Back));
    }
    items.push(("Back".to_string(), MenuAction::Back));
    items
}

fn menu_title(screen: MenuScreen) -> &'static str {
    match screen {
        MenuScreen::Main => "PLATFORMER",
        MenuScreen::NewSlots => "NEW GAME",
        MenuScreen::LoadSlots => "LOAD GAME",
        MenuScreen::ConfirmNew(_) => "OVERWRITE SAVE?",
        MenuScreen::ModeSelect(_) => "CHOOSE MODE",
        MenuScreen::NameEntry(..) => "NAME YOUR SAVE",
        MenuScreen::Options => "OPTIONS",
        MenuScreen::ConfirmQuit => "QUIT GAME?",
    }
}

fn labels(items: &[(String, MenuAction)]) -> Vec<String> {
    items.iter().map(|(label, _)| label.clone()).collect()
}

/// The universal "back / cancel": `Esc` on the keyboard or **Circle** (East) on a gamepad.
/// Used by both menu systems to step back one screen without hunting for the "Back" row.
fn cancel_just_pressed(keys: &ButtonInput<KeyCode>, gamepads: &Query<&Gamepad>) -> bool {
    keys.just_pressed(KeyCode::Escape)
        || gamepads.iter().any(|g| g.just_pressed(GamepadButton::East))
}

/// The icon font ([PromptFont], SIL OFL 1.1) used for control-glyph rows: its ASCII
/// alphabet stays readable while modifier keys and gamepad buttons render as glyphs.
///
/// [PromptFont]: https://shinmera.com/promptfont
#[derive(Resource)]
struct UiFonts {
    prompt: Handle<Font>,
}

/// Marks a [`Text2d`] whose text is icon-font glyphs; [`apply_prompt_font`] swaps the
/// default font for [`UiFonts::prompt`] the moment the row spawns. Used by the pause
/// controls sheet and the in-world bench/chest prompts (see [`crate::world`]).
#[derive(Component)]
pub(crate) struct PromptGlyph;

/// Decode the embedded icon font into an asset once at startup.
fn load_ui_fonts(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    let bytes = crate::world::EMBEDDED_FONTS
        .iter()
        .find(|(name, _)| *name == "promptfont.ttf")
        .map(|(_, bytes)| *bytes)
        .expect("embedded promptfont.ttf");
    let prompt = fonts.add(Font::from_bytes(bytes.to_vec()));
    commands.insert_resource(UiFonts { prompt });
}

/// Point freshly spawned [`PromptGlyph`] rows at the icon font.
fn apply_prompt_font(
    fonts: Option<Res<UiFonts>>,
    mut rows: Query<&mut TextFont, Added<PromptGlyph>>,
) {
    let Some(fonts) = fonts else {
        return;
    };
    for mut font in &mut rows {
        font.font = FontSource::Handle(fonts.prompt.clone());
    }
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Paused>()
            .init_resource::<MenuCursor>()
            .init_resource::<MenuScreen>()
            .init_resource::<PauseScreen>()
            .init_resource::<NewGameName>()
            // Decode the embedded icon font once, and swap it onto any glyph row as it spawns.
            .add_systems(Startup, load_ui_fonts)
            .add_systems(Update, apply_prompt_font.after(pause_menu_update))
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_menu)
            .add_systems(
                Update,
                main_menu_update.run_if(in_state(GameState::MainMenu)),
            )
            .add_systems(
                Update,
                open_pause.run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(Paused::Running))
                        .and_then(in_state(MapView::Closed))
                        .and_then(in_state(crate::stats::CharMenu::Closed)),
                ),
            )
            .add_systems(OnEnter(Paused::Paused), spawn_pause_menu)
            .add_systems(OnExit(Paused::Paused), despawn_menu)
            .add_systems(Update, pause_menu_update.run_if(in_state(Paused::Paused)))
            // Apply the window-mode preference whenever it changes (and once on startup).
            .add_systems(
                Update,
                apply_window_mode.run_if(resource_changed::<Settings>),
            );
    }
}

/// Push the [`Settings`] window mode onto the primary window (windowed ↔ borderless).
fn apply_window_mode(
    settings: Res<Settings>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    let Ok(mut window) = windows.single_mut() else {
        return;
    };
    let mode = if settings.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
}

fn spawn_main_menu(
    mut commands: Commands,
    mut cursor: ResMut<MenuCursor>,
    mut screen: ResMut<MenuScreen>,
    settings: Res<Settings>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    *screen = MenuScreen::Main;
    cursor.0 = 0;
    let items = main_menu_items(*screen, &settings);
    draw_menu(
        &mut commands,
        camera_center(&camera),
        menu_title(*screen),
        &labels(&items),
    );
}

fn spawn_pause_menu(
    mut commands: Commands,
    mut cursor: ResMut<MenuCursor>,
    mut screen: ResMut<PauseScreen>,
    save: Res<Save>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    *screen = PauseScreen::Root; // always open on the root list
    cursor.0 = 0;
    draw_menu(
        &mut commands,
        camera_center(&camera),
        "PAUSED",
        &labels(&pause_menu_items(save.mode == GameMode::Builder)),
    );
}

fn despawn_menu(mut commands: Commands, menu: Query<Entity, With<MenuEntity>>) {
    for entity in &menu {
        commands.entity(entity).despawn();
    }
}

/// Open the pause menu on `Esc` / **Select**. Runs only while **Running** (closing and the
/// back/cancel shortcut are owned by [`pause_menu_update`]), so it can't also resume on the
/// very press that's meant to step back inside the menu.
fn open_pause(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut next: ResMut<NextState<Paused>>,
) {
    let pressed = keys.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::Select));
    if pressed {
        next.set(Paused::Paused);
    }
}

/// Despawn and redraw the title screen for the current `screen`.
fn redraw_main(
    commands: &mut Commands,
    menu: &Query<Entity, With<MenuEntity>>,
    screen: MenuScreen,
    settings: &Settings,
    camera: &Query<&Transform, With<Camera2d>>,
) {
    for entity in menu.iter() {
        commands.entity(entity).despawn();
    }
    let items = main_menu_items(screen, settings);
    draw_menu(
        commands,
        camera_center(camera),
        menu_title(screen),
        &labels(&items),
    );
}

/// Write a fresh, named save to `slot` (overwriting any existing one) and play. A
/// Builder game also seeds a private, editable copy of the shipped levels.
#[allow(clippy::too_many_arguments)] // distinct resources threaded from the menu system
fn start_new_game(
    slot: usize,
    name: String,
    mode: GameMode,
    save_res: &mut Save,
    pending: &mut PendingSpawn,
    level_root: &mut LevelRoot,
    game_state: &mut NextState<GameState>,
) {
    *level_root = match mode {
        GameMode::Story => LevelRoot::Story,
        GameMode::Builder => LevelRoot::Builder(init_builder_world(slot)),
    };
    let fresh = Save {
        slot,
        mode,
        name,
        room: START_MAP.to_string(),
        ..default()
    };
    save::write_save(&fresh);
    *save_res = fresh;
    pending.0 = Some(SpawnRequest {
        room: START_MAP.to_string(),
        at_cell: None,
    });
    game_state.set(GameState::Loading);
}

/// Draw the name-entry screen, showing the name typed so far with a cursor.
fn draw_name_entry(
    commands: &mut Commands,
    menu: &Query<Entity, With<MenuEntity>>,
    camera: &Query<&Transform, With<Camera2d>>,
    name: &str,
) {
    for entity in menu.iter() {
        commands.entity(entity).despawn();
    }
    draw_menu(
        commands,
        camera_center(camera),
        "NAME YOUR SAVE",
        &[
            format!("{name}_"),
            "[Enter] start    [Esc] back".to_string(),
        ],
    );
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn main_menu_update(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut typed: MessageReader<KeyboardInput>,
    mut cursor: ResMut<MenuCursor>,
    mut screen: ResMut<MenuScreen>,
    mut name_buf: ResMut<NewGameName>,
    rows: Query<(&MenuItem, &mut TextColor)>,
    menu: Query<Entity, With<MenuEntity>>,
    camera: Query<&Transform, With<Camera2d>>,
    mut save_res: ResMut<Save>,
    mut pending: ResMut<PendingSpawn>,
    mut level_root: ResMut<LevelRoot>,
    mut game_state: ResMut<NextState<GameState>>,
    mut settings: ResMut<Settings>,
    mut exit: MessageWriter<AppExit>,
) {
    // Always drain typed keys so none are stale when name entry begins.
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();

    // The name-entry screen captures the keyboard; navigation is suspended.
    if let MenuScreen::NameEntry(slot, mode) = *screen {
        let mut changed = false;
        for ev in &events {
            if ev.state != ButtonState::Pressed {
                continue;
            }
            match &ev.logical_key {
                Key::Enter => {
                    let name = name_buf.0.trim().to_string();
                    start_new_game(
                        slot,
                        name,
                        mode,
                        &mut save_res,
                        &mut pending,
                        &mut level_root,
                        &mut game_state,
                    );
                    return;
                }
                Key::Escape => {
                    *screen = MenuScreen::ModeSelect(slot);
                    cursor.0 = 0;
                    redraw_main(&mut commands, &menu, *screen, &settings, &camera);
                    return;
                }
                Key::Backspace => {
                    name_buf.0.pop();
                    changed = true;
                }
                Key::Space if name_buf.0.len() < NAME_MAX => {
                    name_buf.0.push(' ');
                    changed = true;
                }
                Key::Character(s) => {
                    for c in s.chars() {
                        // Keep it printable, and RON-safe (no quote/backslash).
                        if !c.is_control() && c != '"' && c != '\\' && name_buf.0.len() < NAME_MAX {
                            name_buf.0.push(c);
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
        if changed {
            draw_name_entry(&mut commands, &menu, &camera, &name_buf.0);
        }
        return;
    }

    // Esc / Circle steps back one screen (like choosing "Back"); a no-op at the top.
    if cancel_just_pressed(&keys, &gamepads) {
        let parent = match *screen {
            MenuScreen::ConfirmNew(_) | MenuScreen::ModeSelect(_) => MenuScreen::NewSlots,
            _ => MenuScreen::Main,
        };
        if parent != *screen {
            *screen = parent;
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        return;
    }

    let items = main_menu_items(*screen, &settings);
    let Some(choice) = update_menu(&keys, &gamepads, &mut cursor, rows) else {
        return;
    };
    let Some(action) = items.get(choice).map(|(_, action)| *action) else {
        return;
    };

    match action {
        MenuAction::NewGame => {
            *screen = MenuScreen::NewSlots;
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::LoadGame => {
            *screen = MenuScreen::LoadSlots;
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::Back => {
            // Step back one level: mode-pick / overwrite-confirm → slots; else top.
            *screen = match *screen {
                MenuScreen::ConfirmNew(_) | MenuScreen::ModeSelect(_) => MenuScreen::NewSlots,
                _ => MenuScreen::Main,
            };
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::PickSlot(slot) => match *screen {
            MenuScreen::NewSlots => {
                if save::read_slot(slot).is_some() {
                    // Occupied — confirm before erasing it.
                    *screen = MenuScreen::ConfirmNew(slot);
                    cursor.0 = 1; // default to the safe "Back" option
                } else {
                    // Empty slot — choose the mode next.
                    *screen = MenuScreen::ModeSelect(slot);
                    cursor.0 = 0;
                }
                redraw_main(&mut commands, &menu, *screen, &settings, &camera);
            }
            MenuScreen::LoadSlots => {
                // Only act on a slot that actually has a save.
                if let Some(loaded) = save::read_slot(slot) {
                    let at_cell = loaded
                        .has_bench()
                        .then_some((loaded.bench_col, loaded.bench_row));
                    let room = if loaded.has_bench() {
                        loaded.bench_room.clone()
                    } else {
                        loaded.room.clone()
                    };
                    // Point the level loader at this save's world.
                    *level_root = match loaded.mode {
                        GameMode::Story => LevelRoot::Story,
                        GameMode::Builder => LevelRoot::Builder(builder_maps_dir(slot)),
                    };
                    pending.0 = Some(SpawnRequest { room, at_cell });
                    *save_res = loaded;
                    game_state.set(GameState::Loading);
                }
            }
            _ => {}
        },
        MenuAction::ConfirmNew => {
            if let MenuScreen::ConfirmNew(slot) = *screen {
                // Confirmed the overwrite — choose the mode next.
                *screen = MenuScreen::ModeSelect(slot);
                cursor.0 = 0;
                redraw_main(&mut commands, &menu, *screen, &settings, &camera);
            }
        }
        MenuAction::PickMode(mode) => {
            if let MenuScreen::ModeSelect(slot) = *screen {
                // Mode chosen — name the new game, then start.
                *screen = MenuScreen::NameEntry(slot, mode);
                name_buf.0.clear();
                draw_name_entry(&mut commands, &menu, &camera, &name_buf.0);
            }
        }
        MenuAction::OpenOptions => {
            *screen = MenuScreen::Options;
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::SetFullscreen(fs) => {
            settings.fullscreen = fs;
            save::write_settings(&settings);
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::StepFxVolume | MenuAction::StepMusicVolume => {
            if matches!(action, MenuAction::StepFxVolume) {
                settings.fx_volume = step_volume(settings.fx_volume);
            } else {
                settings.music_volume = step_volume(settings.music_volume);
            }
            save::write_settings(&settings);
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::Quit => {
            // Confirm first — guard against a misclick on the title screen.
            *screen = MenuScreen::ConfirmQuit;
            cursor.0 = 1; // default to the safe "Back" option
            redraw_main(&mut commands, &menu, *screen, &settings, &camera);
        }
        MenuAction::QuitConfirmed => {
            exit.write(AppExit::Success);
        }
        // Only produced by the pause menu, handled in `pause_menu_update`.
        MenuAction::Continue
        | MenuAction::Character
        | MenuAction::OpenAbilities
        | MenuAction::ToggleAbility(_)
        | MenuAction::OpenControls
        | MenuAction::OpenEditor
        | MenuAction::MainMenu => {}
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn pause_menu_update(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut cursor: ResMut<MenuCursor>,
    mut screen: ResMut<PauseScreen>,
    rows: Query<(&MenuItem, &mut TextColor)>,
    menu: Query<Entity, With<MenuEntity>>,
    camera: Query<&Transform, With<Camera2d>>,
    mut save: ResMut<Save>,
    mut abilities: ResMut<Abilities>,
    info: CharInfo,
    mut settings: ResMut<Settings>,
    mut next: ResMut<NextState<Paused>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    mut start_in_editor: ResMut<crate::editor::StartInEditor>,
) {
    let builder = save.mode == GameMode::Builder;

    // Esc / Circle steps back one screen (resume from the root); Select closes the menu outright.
    let cancel = cancel_just_pressed(&keys, &gamepads);
    let close = gamepads
        .iter()
        .any(|g| g.just_pressed(GamepadButton::Select));
    if close || (cancel && *screen == PauseScreen::Root) {
        next.set(Paused::Running);
        return;
    }
    if cancel {
        *screen = PauseScreen::Root;
        cursor.0 = 0;
        redraw_pause(
            &mut commands,
            &menu,
            &camera,
            *screen,
            builder,
            &info.stats,
            &info.energy,
            &info.lost,
            &settings,
            &abilities,
            &save,
        );
        return;
    }

    // The sub-screens have their own rows; the root has the full list.
    let items = match *screen {
        PauseScreen::Root => pause_menu_items(builder),
        PauseScreen::Character => vec![("Back".to_string(), MenuAction::Back)],
        PauseScreen::Options => options_items(&settings),
        PauseScreen::Abilities => ability_items(builder, &abilities, &save),
        PauseScreen::Controls => vec![("Back".to_string(), MenuAction::Back)],
        PauseScreen::ConfirmQuit => vec![
            ("Quit game".to_string(), MenuAction::QuitConfirmed),
            ("Back".to_string(), MenuAction::Back),
        ],
    };
    let Some(choice) = update_menu(&keys, &gamepads, &mut cursor, rows) else {
        return;
    };
    match items.get(choice).map(|(_, action)| *action) {
        Some(MenuAction::Continue) => next.set(Paused::Running),
        Some(MenuAction::Character) => {
            *screen = PauseScreen::Character;
            cursor.0 = 0;
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::Back) => {
            *screen = PauseScreen::Root;
            cursor.0 = 0;
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::OpenOptions) => {
            *screen = PauseScreen::Options;
            cursor.0 = 0;
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::OpenAbilities) => {
            *screen = PauseScreen::Abilities;
            cursor.0 = 0;
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::OpenControls) => {
            *screen = PauseScreen::Controls;
            cursor.0 = 0;
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::ToggleAbility(ability)) => {
            // Builder: grant/remove (persisted). Story: flip the live "active" state of an
            // already-acquired ability (the save's acquired set is untouched).
            let on = !abilities.has(ability);
            abilities.set(ability, on);
            if builder {
                save.abilities = abilities.to_csv();
                save::write_save(&save);
            }
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::SetFullscreen(fs)) => {
            settings.fullscreen = fs;
            save::write_settings(&settings);
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(action @ (MenuAction::StepFxVolume | MenuAction::StepMusicVolume)) => {
            if matches!(action, MenuAction::StepFxVolume) {
                settings.fx_volume = step_volume(settings.fx_volume);
            } else {
                settings.music_volume = step_volume(settings.music_volume);
            }
            save::write_settings(&settings);
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::OpenEditor) => {
            next.set(Paused::Running); // resume, then `editor` jumps in
            start_in_editor.0 = true;
        }
        Some(MenuAction::MainMenu) => {
            // Resume (so pause systems stop) and drop back to the title screen.
            next.set(Paused::Running);
            game_state.set(GameState::MainMenu);
        }
        Some(MenuAction::Quit) => {
            // Confirm first — guard against a misclick.
            *screen = PauseScreen::ConfirmQuit;
            cursor.0 = 1; // default to the safe "Back" option
            redraw_pause(
                &mut commands,
                &menu,
                &camera,
                *screen,
                builder,
                &info.stats,
                &info.energy,
                &info.lost,
                &settings,
                &abilities,
                &save,
            );
        }
        Some(MenuAction::QuitConfirmed) => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

/// Despawn and redraw the pause overlay for the current sub-screen.
#[allow(clippy::too_many_arguments)] // distinct queries/resources, threaded to the draw
fn redraw_pause(
    commands: &mut Commands,
    menu: &Query<Entity, With<MenuEntity>>,
    camera: &Query<&Transform, With<Camera2d>>,
    screen: PauseScreen,
    builder: bool,
    stats: &Stats,
    energy: &Energy,
    lost: &LostEnergy,
    settings: &Settings,
    abilities: &Abilities,
    save: &Save,
) {
    for entity in menu.iter() {
        commands.entity(entity).despawn();
    }
    let center = camera_center(camera);
    match screen {
        PauseScreen::Root => draw_menu(
            commands,
            center,
            "PAUSED",
            &labels(&pause_menu_items(builder)),
        ),
        PauseScreen::Character => {
            draw_character_sheet(commands, center, &character_lines(stats, energy, lost));
        }
        PauseScreen::Options => {
            draw_menu(
                commands,
                center,
                "OPTIONS",
                &labels(&options_items(settings)),
            );
        }
        PauseScreen::Abilities => {
            draw_menu(
                commands,
                center,
                "ABILITIES",
                &labels(&ability_items(builder, abilities, save)),
            );
        }
        PauseScreen::Controls => draw_controls_sheet(commands, center, builder, save),
        PauseScreen::ConfirmQuit => draw_menu(
            commands,
            center,
            "QUIT GAME?",
            &["Quit game".to_string(), "Back".to_string()],
        ),
    }
}

/// X columns (relative to centre) for the label / keyboard / controller pieces of a row.
const CTRL_LABEL_X: f32 = -460.0;
const CTRL_KEY_X: f32 = 120.0;
const CTRL_PAD_X: f32 = 300.0;

/// Draw the read-only controls reference: a title, two glyph columns (keyboard | controller)
/// grouped into sections, and a selectable "Back" row. The key/pad tokens render in the icon
/// font; unlockable abilities are gated in Story (see [`control_sections`]).
fn draw_controls_sheet(commands: &mut Commands, center: Vec2, builder: bool, save: &Save) {
    commands.spawn((
        MenuEntity,
        Sprite {
            color: Color::srgba(0.03, 0.03, 0.06, 0.97),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 200.0),
    ));
    commands.spawn((
        MenuEntity,
        Text2d::new("CONTROLS"),
        TextFont {
            font_size: FontSize::Px(30.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Transform::from_xyz(center.x, center.y + 246.0, 201.0),
    ));
    // Column captions over the two glyph columns.
    for (x, cap) in [(CTRL_KEY_X, "keyboard"), (CTRL_PAD_X, "controller")] {
        commands.spawn((
            MenuEntity,
            Text2d::new(cap),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgb(0.5, 0.53, 0.62)),
            Transform::from_xyz(center.x + x, center.y + 214.0, 201.0),
        ));
    }

    let mut y = center.y + 188.0;
    for (header, rows) in control_sections(builder, save) {
        commands.spawn((
            MenuEntity,
            Text2d::new(header),
            TextFont {
                font_size: FontSize::Px(16.0),
                ..default()
            },
            TextColor(Color::srgb(0.95, 0.82, 0.45)),
            Anchor::CENTER_LEFT,
            Transform::from_xyz(center.x + CTRL_LABEL_X, y, 201.0),
        ));
        y -= 27.0;
        for row in rows {
            commands.spawn((
                MenuEntity,
                Text2d::new(row.label),
                TextFont {
                    font_size: FontSize::Px(17.0),
                    ..default()
                },
                TextColor(Color::srgb(0.78, 0.8, 0.88)),
                Anchor::CENTER_LEFT,
                Transform::from_xyz(center.x + CTRL_LABEL_X + 20.0, y, 201.0),
            ));
            for (x, token) in [(CTRL_KEY_X, row.key), (CTRL_PAD_X, row.pad)] {
                commands.spawn((
                    MenuEntity,
                    PromptGlyph,
                    Text2d::new(token),
                    TextFont {
                        font_size: FontSize::Px(23.0),
                        ..default()
                    },
                    TextColor(Color::srgb(0.86, 0.89, 1.0)),
                    Transform::from_xyz(center.x + x, y, 201.0),
                ));
            }
            y -= 26.0;
        }
        y -= 6.0;
    }

    commands.spawn((
        MenuEntity,
        MenuItem(0),
        Text2d::new("Back"),
        TextFont {
            font_size: FontSize::Px(24.0),
            ..default()
        },
        TextColor(Color::srgb(0.62, 0.64, 0.72)),
        Transform::from_xyz(center.x, center.y - 244.0, 201.0),
    ));
}

/// Draw the read-only character status sheet: a title, the stat/energy lines, and a
/// single selectable "Back" row.
fn draw_character_sheet(commands: &mut Commands, center: Vec2, info: &[String]) {
    commands.spawn((
        MenuEntity,
        Sprite {
            color: Color::srgba(0.03, 0.03, 0.06, 0.97),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 200.0),
    ));
    commands.spawn((
        MenuEntity,
        Text2d::new("CHARACTER"),
        TextFont {
            font_size: FontSize::Px(44.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Transform::from_xyz(center.x, center.y + 160.0, 201.0),
    ));
    for (i, line) in info.iter().enumerate() {
        commands.spawn((
            MenuEntity,
            Text2d::new(line.clone()),
            TextFont {
                font_size: FontSize::Px(24.0),
                ..default()
            },
            TextColor(Color::srgb(0.78, 0.8, 0.88)),
            Transform::from_xyz(center.x, center.y + 90.0 - i as f32 * 34.0, 201.0),
        ));
    }
    commands.spawn((
        MenuEntity,
        MenuItem(0),
        Text2d::new("Back"),
        TextFont {
            font_size: FontSize::Px(30.0),
            ..default()
        },
        TextColor(Color::srgb(0.62, 0.64, 0.72)),
        Transform::from_xyz(center.x, center.y - 190.0, 201.0),
    ));
}

/// Move the cursor, recolour the rows, and return the chosen index on confirm.
fn update_menu(
    keys: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    cursor: &mut MenuCursor,
    mut items: Query<(&MenuItem, &mut TextColor)>,
) -> Option<usize> {
    let count = items.iter().count().max(1) as i32;
    let up = keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::DPadUp));
    let down = keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::DPadDown));
    let confirm = keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, KeyCode::KeyZ])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::South));

    let delta = i32::from(down) - i32::from(up);
    if delta != 0 {
        cursor.0 = (cursor.0 as i32 + delta).rem_euclid(count) as usize;
    }

    for (item, mut color) in &mut items {
        *color = TextColor(if item.0 == cursor.0 {
            Color::srgb(1.0, 0.85, 0.3)
        } else {
            Color::srgb(0.62, 0.64, 0.72)
        });
    }

    confirm.then_some(cursor.0)
}

fn draw_menu(commands: &mut Commands, center: Vec2, title: &str, items: &[String]) {
    commands.spawn((
        MenuEntity,
        Sprite {
            color: Color::srgba(0.03, 0.03, 0.06, 0.97),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 200.0),
    ));
    // Tighten the rows for long lists (the 10-slot picker) so they all fit; the
    // list is centred and the title sits just above it.
    let n = items.len();
    let (font, step, title_font) = if n > 6 {
        (22.0, 38.0, 34.0)
    } else {
        (32.0, 52.0, 44.0)
    };
    let top = (n.saturating_sub(1)) as f32 * step / 2.0;
    commands.spawn((
        MenuEntity,
        Text2d::new(title),
        TextFont {
            font_size: FontSize::Px(title_font),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Transform::from_xyz(center.x, center.y + top + 48.0, 201.0),
    ));
    for (i, label) in items.iter().enumerate() {
        commands.spawn((
            MenuEntity,
            MenuItem(i),
            Text2d::new(label.clone()),
            TextFont {
                font_size: FontSize::Px(font),
                ..default()
            },
            TextColor(Color::srgb(0.62, 0.64, 0.72)),
            Transform::from_xyz(center.x, center.y + top - i as f32 * step, 201.0),
        ));
    }
}

fn camera_center(camera: &Query<&Transform, With<Camera2d>>) -> Vec2 {
    camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO)
}
