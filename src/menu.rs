//! Main menu (title screen) and pause menu.
//!
//! The game boots into [`GameState::MainMenu`]. The title screen has a few screens:
//! the main list, and a save-slot picker for **New Game** / **Load Game** (a
//! three-slot system — see [`crate::save`]). During play, `Esc` (or the gamepad
//! `Select` button) toggles a [`Paused`] overlay (Continue / Quit); gameplay is
//! frozen while paused (the gameplay [`GameSet`](crate::GameSet) chain is gated on
//! [`Paused::Running`]).
//!
//! Menus are drawn the lightweight way the world map is — a backdrop sprite plus
//! `Text2d` rows around the camera — and navigated with the arrows / D-pad,
//! confirmed with jump / `Enter` / `South`.

use bevy::app::AppExit;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::save::{self, SLOTS, Save};
use crate::state::GameState;
use crate::world::{PendingSpawn, START_MAP, SpawnRequest};
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
    /// Type a name for the new game in this slot, then start.
    NameEntry(usize),
}

/// The name being typed for a new game (in the [`MenuScreen::NameEntry`] screen).
#[derive(Resource, Default)]
struct NewGameName(String);

/// Max length of a save name.
const NAME_MAX: usize = 20;

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
    Back,
    Continue,
    /// Leave a paused game back to the title screen.
    MainMenu,
    Quit,
    /// Jump into the level builder — only offered in debug builds.
    #[cfg(debug_assertions)]
    OpenEditor,
}

fn main_menu_items(screen: MenuScreen) -> Vec<(String, MenuAction)> {
    match screen {
        MenuScreen::Main => {
            let mut items = vec![
                ("New Game".to_string(), MenuAction::NewGame),
                ("Load Game".to_string(), MenuAction::LoadGame),
            ];
            #[cfg(debug_assertions)]
            items.push(("Level Builder".to_string(), MenuAction::OpenEditor));
            items.push(("Quit".to_string(), MenuAction::Quit));
            items
        }
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
        // Drawn specially (it needs the live name buffer) — see `draw_name_entry`.
        MenuScreen::NameEntry(_) => Vec::new(),
    }
}

/// A one-line summary of a save slot for the picker.
fn slot_label(slot: usize) -> String {
    let detail = match save::read_slot(slot) {
        Some(s) if !s.name.is_empty() => s.name,
        Some(s) if s.has_bench() => format!("bench in {}", s.bench_room),
        Some(_) => "new game".to_string(),
        None => "empty".to_string(),
    };
    format!("Slot {} - {}", slot + 1, detail)
}

fn pause_menu_items() -> Vec<(String, MenuAction)> {
    let mut items = vec![("Continue".to_string(), MenuAction::Continue)];
    #[cfg(debug_assertions)]
    items.push(("Level Builder".to_string(), MenuAction::OpenEditor));
    items.push(("Main Menu".to_string(), MenuAction::MainMenu));
    items.push(("Quit".to_string(), MenuAction::Quit));
    items
}

fn menu_title(screen: MenuScreen) -> &'static str {
    match screen {
        MenuScreen::Main => "PLATFORMER",
        MenuScreen::NewSlots => "NEW GAME",
        MenuScreen::LoadSlots => "LOAD GAME",
        MenuScreen::ConfirmNew(_) => "OVERWRITE SAVE?",
        MenuScreen::NameEntry(_) => "NAME YOUR SAVE",
    }
}

fn labels(items: &[(String, MenuAction)]) -> Vec<String> {
    items.iter().map(|(label, _)| label.clone()).collect()
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Paused>()
            .init_resource::<MenuCursor>()
            .init_resource::<MenuScreen>()
            .init_resource::<NewGameName>()
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_menu)
            .add_systems(
                Update,
                main_menu_update.run_if(in_state(GameState::MainMenu)),
            )
            .add_systems(
                Update,
                toggle_pause.run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(MapView::Closed))
                        .and_then(in_state(crate::stats::CharMenu::Closed)),
                ),
            )
            .add_systems(OnEnter(Paused::Paused), spawn_pause_menu)
            .add_systems(OnExit(Paused::Paused), despawn_menu)
            .add_systems(Update, pause_menu_update.run_if(in_state(Paused::Paused)));
    }
}

fn spawn_main_menu(
    mut commands: Commands,
    mut cursor: ResMut<MenuCursor>,
    mut screen: ResMut<MenuScreen>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    *screen = MenuScreen::Main;
    cursor.0 = 0;
    let items = main_menu_items(*screen);
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
    camera: Query<&Transform, With<Camera2d>>,
) {
    cursor.0 = 0;
    draw_menu(
        &mut commands,
        camera_center(&camera),
        "PAUSED",
        &labels(&pause_menu_items()),
    );
}

fn despawn_menu(mut commands: Commands, menu: Query<Entity, With<MenuEntity>>) {
    for entity in &menu {
        commands.entity(entity).despawn();
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    state: Res<State<Paused>>,
    mut next: ResMut<NextState<Paused>>,
) {
    let pressed = keys.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::Select));
    if pressed {
        next.set(match state.get() {
            Paused::Running => Paused::Paused,
            Paused::Paused => Paused::Running,
        });
    }
}

/// Despawn and redraw the title screen for the current `screen`.
fn redraw_main(
    commands: &mut Commands,
    menu: &Query<Entity, With<MenuEntity>>,
    screen: MenuScreen,
    camera: &Query<&Transform, With<Camera2d>>,
) {
    for entity in menu.iter() {
        commands.entity(entity).despawn();
    }
    let items = main_menu_items(screen);
    draw_menu(
        commands,
        camera_center(camera),
        menu_title(screen),
        &labels(&items),
    );
}

/// Write a fresh, named save to `slot` (overwriting any existing one) and play.
fn start_new_game(
    slot: usize,
    name: String,
    save_res: &mut Save,
    pending: &mut PendingSpawn,
    game_state: &mut NextState<GameState>,
) {
    let fresh = Save {
        slot,
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
    mut game_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    #[cfg(debug_assertions)] mut start_in_editor: ResMut<crate::editor::StartInEditor>,
) {
    // Always drain typed keys so none are stale when name entry begins.
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();

    // The name-entry screen captures the keyboard; navigation is suspended.
    if let MenuScreen::NameEntry(slot) = *screen {
        let mut changed = false;
        for ev in &events {
            if ev.state != ButtonState::Pressed {
                continue;
            }
            match &ev.logical_key {
                Key::Enter => {
                    let name = name_buf.0.trim().to_string();
                    start_new_game(slot, name, &mut save_res, &mut pending, &mut game_state);
                    return;
                }
                Key::Escape => {
                    *screen = MenuScreen::NewSlots;
                    cursor.0 = 0;
                    redraw_main(&mut commands, &menu, *screen, &camera);
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

    let items = main_menu_items(*screen);
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
            redraw_main(&mut commands, &menu, *screen, &camera);
        }
        MenuAction::LoadGame => {
            *screen = MenuScreen::LoadSlots;
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &camera);
        }
        MenuAction::Back => {
            // From a confirm prompt, step back to slot selection; else to the top.
            *screen = match *screen {
                MenuScreen::ConfirmNew(_) => MenuScreen::NewSlots,
                _ => MenuScreen::Main,
            };
            cursor.0 = 0;
            redraw_main(&mut commands, &menu, *screen, &camera);
        }
        MenuAction::PickSlot(slot) => match *screen {
            MenuScreen::NewSlots => {
                if save::read_slot(slot).is_some() {
                    // Occupied — confirm before erasing it.
                    *screen = MenuScreen::ConfirmNew(slot);
                    cursor.0 = 1; // default to the safe "Back" option
                    redraw_main(&mut commands, &menu, *screen, &camera);
                } else {
                    // Empty slot — name the new game, then start.
                    *screen = MenuScreen::NameEntry(slot);
                    name_buf.0.clear();
                    draw_name_entry(&mut commands, &menu, &camera, &name_buf.0);
                }
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
                    pending.0 = Some(SpawnRequest { room, at_cell });
                    *save_res = loaded;
                    game_state.set(GameState::Loading);
                }
            }
            _ => {}
        },
        MenuAction::ConfirmNew => {
            if let MenuScreen::ConfirmNew(slot) = *screen {
                // Confirmed the overwrite — name the new game, then start.
                *screen = MenuScreen::NameEntry(slot);
                name_buf.0.clear();
                draw_name_entry(&mut commands, &menu, &camera, &name_buf.0);
            }
        }
        MenuAction::Quit => {
            exit.write(AppExit::Success);
        }
        #[cfg(debug_assertions)]
        MenuAction::OpenEditor => {
            game_state.set(GameState::Loading); // load assets, then `editor` jumps in
            start_in_editor.0 = true;
        }
        // Only produced by the pause menu, handled in `pause_menu_update`.
        MenuAction::Continue | MenuAction::MainMenu => {}
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn pause_menu_update(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut cursor: ResMut<MenuCursor>,
    rows: Query<(&MenuItem, &mut TextColor)>,
    mut next: ResMut<NextState<Paused>>,
    mut game_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    #[cfg(debug_assertions)] mut start_in_editor: ResMut<crate::editor::StartInEditor>,
) {
    let items = pause_menu_items();
    let Some(choice) = update_menu(&keys, &gamepads, &mut cursor, rows) else {
        return;
    };
    match items.get(choice).map(|(_, action)| *action) {
        Some(MenuAction::Continue) => next.set(Paused::Running),
        #[cfg(debug_assertions)]
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
            exit.write(AppExit::Success);
        }
        _ => {}
    }
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
    commands.spawn((
        MenuEntity,
        Text2d::new(title),
        TextFont {
            font_size: FontSize::Px(46.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Transform::from_xyz(center.x, center.y + 130.0, 201.0),
    ));
    for (i, label) in items.iter().enumerate() {
        commands.spawn((
            MenuEntity,
            MenuItem(i),
            Text2d::new(label.clone()),
            TextFont {
                font_size: FontSize::Px(32.0),
                ..default()
            },
            TextColor(Color::srgb(0.62, 0.64, 0.72)),
            Transform::from_xyz(center.x, center.y + 30.0 - i as f32 * 52.0, 201.0),
        ));
    }
}

fn camera_center(camera: &Query<&Transform, With<Camera2d>>) -> Vec2 {
    camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO)
}
