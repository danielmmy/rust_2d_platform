//! Character stats, the bench **shop**, and the **character screen**.
//!
//! The player has three Dark-Souls-flavoured stats — **Vitality** (hit points),
//! **Strength** (sword damage) and **Poise** (shorter stagger when hit) — each a
//! level from 1 up. Spending [`Energy`](crate::combat::Energy) raises a level; the
//! cost climbs with each one ([`upgrade_cost`]).
//!
//! One overlay serves two roles ([`OverlayMode`]): pressing **`C`** anywhere opens a
//! read-only **character screen**; interacting with a **bench** opens the **shop**,
//! where you can *Rest* (save, restore, respawn), buy upgrades, or *Leave*.
//! Progression is banked into the [`Save`] at rest/upgrade/death points.
//!
//! Death drops **all** carried energy as a [`Bloodstain`](crate::combat::Bloodstain)
//! at the spot you fell ([`on_player_death`]); reach it before dying again to get it
//! back, or lose it for good.

use bevy::prelude::*;

use crate::audio::{PlaySfx, Sfx};
use crate::combat::{Energy, LostEnergy};
use crate::health::Health;
use crate::input::LastInput;
use crate::menu::Paused;
use crate::player::{Abilities, Ability};
use crate::save::{self, Save};
use crate::state::GameState;
use crate::world::{CurrentRoom, Entry, LoadMap};
use crate::worldmap::MapView;

/// Base values at stat level 1.
const BASE_HEARTS: i32 = 3;
const BASE_DAMAGE: i32 = 1;
/// Highest level any stat can reach.
const LEVEL_MAX: u32 = 10;
/// Energy to raise a stat one level is `UPGRADE_BASE * current_level`, so each
/// level costs more than the last.
const UPGRADE_BASE: u32 = 5;

/// The player's character stats (levels; base 1). Derived values come from the
/// getters so the rest of the game reads effects, not raw levels.
#[derive(Resource)]
pub struct Stats {
    pub vitality: u32,
    pub strength: u32,
    pub poise: u32,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            vitality: 1,
            strength: 1,
            poise: 1,
        }
    }
}

impl Stats {
    /// Maximum hearts, from Vitality.
    pub fn max_hearts(&self) -> i32 {
        BASE_HEARTS + self.vitality.max(1) as i32 - 1
    }
    /// Sword damage per hit, from Strength.
    pub fn sword_damage(&self) -> i32 {
        BASE_DAMAGE + self.strength.max(1) as i32 - 1
    }
    /// Multiplier on hit-stun, from Poise (higher Poise → shorter stagger).
    pub fn stun_scale(&self) -> f32 {
        1.0 / (1.0 + 0.30 * self.poise.saturating_sub(1) as f32)
    }
}

/// Energy to raise a stat from `level` to `level + 1`.
pub fn upgrade_cost(level: u32) -> u32 {
    UPGRADE_BASE * level
}

/// One upgradable stat, in character-screen order.
#[derive(Clone, Copy)]
enum Stat {
    Vitality,
    Strength,
    Poise,
}

const STATS_ORDER: [Stat; 3] = [Stat::Vitality, Stat::Strength, Stat::Poise];

impl Stat {
    fn name(self) -> &'static str {
        match self {
            Stat::Vitality => "Vitality",
            Stat::Strength => "Strength",
            Stat::Poise => "Poise",
        }
    }
    fn level(self, s: &Stats) -> u32 {
        match self {
            Stat::Vitality => s.vitality,
            Stat::Strength => s.strength,
            Stat::Poise => s.poise,
        }
    }
    fn level_mut(self, s: &mut Stats) -> &mut u32 {
        match self {
            Stat::Vitality => &mut s.vitality,
            Stat::Strength => &mut s.strength,
            Stat::Poise => &mut s.poise,
        }
    }
    /// A short description of the stat's current effect.
    fn effect(self, s: &Stats) -> String {
        match self {
            Stat::Vitality => format!("{} HP", s.max_hearts()),
            Stat::Strength => format!("{} ATK", s.sword_damage()),
            Stat::Poise => format!("{}% stagger", (s.stun_scale() * 100.0).round() as i32),
        }
    }
}

/// The read-only character sheet as display lines: a row per stat, then energy, then
/// any pending bloodstain. Shared by the `C` overlay and the pause menu's Character
/// sub-screen so both stay in sync.
pub(crate) fn character_lines(stats: &Stats, energy: &Energy, lost: &LostEnergy) -> Vec<String> {
    let mut lines: Vec<String> = STATS_ORDER
        .iter()
        .map(|&stat| {
            format!(
                "{:9} Lv {:>2}   {}",
                stat.name(),
                stat.level(stats),
                stat.effect(stats)
            )
        })
        .collect();
    lines.push(String::new());
    lines.push(format!("Energy: {}", energy.0));
    if lost.amount > 0 {
        lines.push(format!("Lost {} energy in {}", lost.amount, lost.room));
    }
    lines
}

/// Whether the character screen is open. Gameplay freezes while it is.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CharMenu {
    #[default]
    Closed,
    Open,
}

/// Emitted by [`crate::health`] when the player runs out of hearts, carrying where
/// (and in which room) they fell so a bloodstain can be dropped there.
#[derive(Message, Clone)]
pub(crate) struct Died {
    pub pos: Vec2,
    pub room: String,
}

/// Copy the live progression resources into the [`Save`] and write it to disk.
/// Called at every save point (resting, upgrading, dying).
pub(crate) fn write_progress(save: &mut Save, energy: &Energy, stats: &Stats, lost: &LostEnergy) {
    save.energy = energy.0;
    save.vitality = stats.vitality;
    save.strength = stats.strength;
    save.poise = stats.poise;
    save.lost_amount = lost.amount;
    save.lost_room = lost.room.clone();
    save.lost_x = lost.pos.x;
    save.lost_y = lost.pos.y;
    save::write_save(save);
}

pub struct StatsPlugin;

impl Plugin for StatsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Stats>()
            .init_resource::<CharCursor>()
            .init_resource::<OverlayMode>()
            .init_resource::<BenchAt>()
            .init_state::<CharMenu>()
            .add_message::<Died>()
            .add_systems(OnEnter(GameState::Playing), apply_save_to_session)
            .add_systems(Update, on_player_death.run_if(in_state(GameState::Playing)))
            .add_systems(
                Update,
                toggle_char_menu.run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(MapView::Closed))
                        .and_then(in_state(Paused::Running))
                        .and_then(in_state(CharMenu::Closed)),
                ),
            )
            .add_systems(OnEnter(CharMenu::Open), open_overlay)
            .add_systems(OnExit(CharMenu::Open), despawn_char_menu)
            .add_systems(
                Update,
                (update_overlay, refresh_ability_list, refresh_hint)
                    .run_if(in_state(CharMenu::Open)),
            );
    }
}

/// On entering play (after loading a save or starting fresh), push the saved
/// progression into the live resources and size health from Vitality.
fn apply_save_to_session(
    save: Res<Save>,
    mut stats: ResMut<Stats>,
    mut energy: ResMut<Energy>,
    mut lost: ResMut<LostEnergy>,
    mut health: ResMut<Health>,
) {
    stats.vitality = save.vitality.max(1);
    stats.strength = save.strength.max(1);
    stats.poise = save.poise.max(1);
    energy.0 = save.energy;
    lost.amount = save.lost_amount;
    lost.room = save.lost_room.clone();
    lost.pos = Vec2::new(save.lost_x, save.lost_y);
    health.max = stats.max_hearts();
    health.current = health.max;
}

/// When the player dies, drop **all** carried energy as a bloodstain where they
/// fell (replacing any older one — that one is lost for good) and persist it.
fn on_player_death(
    mut died: MessageReader<Died>,
    mut energy: ResMut<Energy>,
    mut lost: ResMut<LostEnergy>,
    stats: Res<Stats>,
    mut save: ResMut<Save>,
) {
    let Some(death) = died.read().last().cloned() else {
        return;
    };
    lost.amount = energy.0;
    lost.room = death.room;
    lost.pos = death.pos;
    energy.0 = 0;
    write_progress(&mut save, &energy, &stats, &lost);
}

// --- character screen / bench shop ---------------------------------------

/// Which flavour of the overlay is showing.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayMode {
    /// Opened with `C`: a read-only character sheet (upgrades happen at benches).
    #[default]
    Character,
    /// Opened at a bench: the shop — Rest, buy upgrades, or Leave.
    Bench,
}

/// The bench cell (`col`, `row`) the player opened the shop at, used by *Rest* to
/// reload the room and record the checkpoint. Set by [`crate::world`].
#[derive(Resource, Default)]
pub(crate) struct BenchAt(pub(crate) Option<(i32, i32)>);

/// Tags every entity that makes up the overlay.
#[derive(Component)]
struct CharEntity;

/// The character screen's read-only "acquired abilities" line.
#[derive(Component)]
struct AbilityList;

/// The overlay's control-hint line (refreshed to the last-used input device).
#[derive(Component)]
struct HintLine;

/// A live line on the overlay, refreshed each frame.
#[derive(Component)]
enum Line {
    /// A selectable option (index into the current mode's [`choices`]).
    Option(usize),
    Energy,
    Lost,
}

/// The highlighted option row.
#[derive(Resource, Default)]
struct CharCursor(usize);

/// One selectable thing on the overlay.
#[derive(Clone, Copy)]
enum Choice {
    Rest,
    /// Buy a level of a stat (index into [`STATS_ORDER`]).
    Buy(usize),
    Leave,
}

/// The options shown for a mode: the character sheet just lists stats; the bench
/// shop wraps them with *Rest* and *Leave*.
fn choices(mode: OverlayMode) -> Vec<Choice> {
    let stats = (0..STATS_ORDER.len()).map(Choice::Buy);
    match mode {
        OverlayMode::Character => stats.collect(),
        OverlayMode::Bench => std::iter::once(Choice::Rest)
            .chain(stats)
            .chain(std::iter::once(Choice::Leave))
            .collect(),
    }
}

impl Choice {
    fn label(self, stats: &Stats) -> String {
        match self {
            Choice::Rest => "Rest  (save & restore)".to_string(),
            Choice::Leave => "Leave".to_string(),
            Choice::Buy(i) => {
                let stat = STATS_ORDER[i];
                let level = stat.level(stats);
                let next_cost = if level >= LEVEL_MAX {
                    "MAX".to_string()
                } else {
                    format!("{} energy", upgrade_cost(level))
                };
                format!(
                    "{:9} Lv {:>2}   {:11}   next: {}",
                    stat.name(),
                    level,
                    stat.effect(stats),
                    next_cost
                )
            }
        }
    }
}

fn toggle_char_menu(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut mode: ResMut<OverlayMode>,
    mut next: ResMut<NextState<CharMenu>>,
) {
    let pressed = keys.just_pressed(KeyCode::KeyC)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::LeftTrigger));
    if pressed {
        *mode = OverlayMode::Character;
        next.set(CharMenu::Open);
    }
}

fn open_overlay(
    mut commands: Commands,
    mode: Res<OverlayMode>,
    mut cursor: ResMut<CharCursor>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    cursor.0 = 0;
    let center = camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO);

    let title = match *mode {
        OverlayMode::Character => "CHARACTER",
        OverlayMode::Bench => "BENCH",
    };

    commands.spawn((
        CharEntity,
        Sprite {
            color: Color::srgba(0.03, 0.03, 0.06, 0.97),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 200.0),
    ));
    spawn_line(&mut commands, center, 160.0, 40.0, title, None);

    for i in 0..choices(*mode).len() {
        let y = 100.0 - i as f32 * 40.0;
        spawn_line(&mut commands, center, y, 26.0, "", Some(Line::Option(i)));
    }
    spawn_line(&mut commands, center, -120.0, 24.0, "", Some(Line::Energy));
    spawn_line(&mut commands, center, -152.0, 18.0, "", Some(Line::Lost));
    // The control hint refreshes each frame to match the last-used device (see `refresh_hint`).
    commands.spawn((
        CharEntity,
        HintLine,
        Text2d::new(String::new()),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::srgb(0.62, 0.64, 0.72)),
        Transform::from_xyz(center.x, center.y - 195.0, 201.0),
    ));

    // The character sheet lists acquired abilities (read-only; refreshed each frame).
    if *mode == OverlayMode::Character {
        commands.spawn((
            CharEntity,
            AbilityList,
            Text2d::new(String::new()),
            TextFont {
                font_size: FontSize::Px(16.0),
                ..default()
            },
            TextColor(Color::srgb(0.7, 0.85, 0.95)),
            Transform::from_xyz(center.x, center.y - 80.0, 201.0),
        ));
    }
}

/// Refresh the character sheet's acquired-abilities line.
fn refresh_ability_list(
    abilities: Res<Abilities>,
    mut line: Query<&mut Text2d, With<AbilityList>>,
) {
    let Ok(mut text) = line.single_mut() else {
        return;
    };
    let names: Vec<&str> = Ability::ALL
        .iter()
        .filter(|a| abilities.has(**a))
        .map(|a| a.label())
        .collect();
    text.0 = if names.is_empty() {
        "Abilities: none yet".to_string()
    } else {
        format!("Abilities: {}", names.join(", "))
    };
}

/// Refresh the overlay's control hint to match the last-used input device.
fn refresh_hint(
    last: Res<LastInput>,
    mode: Res<OverlayMode>,
    mut line: Query<&mut Text2d, With<HintLine>>,
) {
    let Ok(mut text) = line.single_mut() else {
        return;
    };
    text.0 = match *mode {
        OverlayMode::Character => format!("upgrade at a bench    [{}] close", last.cancel()),
        OverlayMode::Bench => format!(
            "[{}] select   [{}] choose   [{}] leave",
            last.updown(),
            last.confirm(),
            last.cancel()
        ),
    };
}

/// Spawn one overlay text line; `live` tags it for per-frame refresh.
fn spawn_line(
    commands: &mut Commands,
    center: Vec2,
    dy: f32,
    size: f32,
    text: &str,
    live: Option<Line>,
) {
    let mut entity = commands.spawn((
        CharEntity,
        Text2d::new(text.to_string()),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.8, 0.82, 0.9)),
        Transform::from_xyz(center.x, center.y + dy, 201.0),
    ));
    if let Some(tag) = live {
        entity.insert(tag);
    }
}

fn despawn_char_menu(mut commands: Commands, items: Query<Entity, With<CharEntity>>) {
    for entity in &items {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn update_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mode: Res<OverlayMode>,
    bench_at: Res<BenchAt>,
    current: Res<CurrentRoom>,
    mut cursor: ResMut<CharCursor>,
    mut energy: ResMut<Energy>,
    mut stats: ResMut<Stats>,
    mut health: ResMut<Health>,
    mut save: ResMut<Save>,
    lost: Res<LostEnergy>,
    mut arenas: ResMut<crate::boss::ClearedArenas>,
    mut next: ResMut<NextState<CharMenu>>,
    mut load: MessageWriter<LoadMap>,
    mut sfx: MessageWriter<PlaySfx>,
    mut lines: Query<(&Line, &mut Text2d, &mut TextColor)>,
) {
    let options = choices(*mode);
    let count = options.len() as i32;
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
    let close = keys.any_just_pressed([KeyCode::KeyC, KeyCode::Escape])
        || gamepads.iter().any(|g| {
            g.just_pressed(GamepadButton::LeftTrigger) || g.just_pressed(GamepadButton::East)
        });

    if close {
        next.set(CharMenu::Closed);
        return;
    }

    let delta = i32::from(down) - i32::from(up);
    if delta != 0 {
        cursor.0 = (cursor.0 as i32 + delta).rem_euclid(count) as usize;
    }

    // Only the bench shop acts on a choice; the `C` sheet is read-only.
    if confirm && *mode == OverlayMode::Bench {
        match options[cursor.0] {
            Choice::Rest => {
                if let Some((col, row)) = bench_at.0 {
                    health.current = health.max;
                    save.room = current.name.clone();
                    save.bench_room = current.name.clone();
                    save.bench_col = col;
                    save.bench_row = row;
                    write_progress(&mut save, &energy, &stats, &lost);
                    // Resting re-arms every non-boss arena (their foes respawn).
                    arenas.0.clear();
                    // Reload the room so enemies respawn and the player lands on the bench.
                    load.write(LoadMap {
                        map: current.name.clone(),
                        entry: Entry::Bench(col, row),
                    });
                    // A little jingle confirming the save (& restore).
                    sfx.write(PlaySfx(Sfx::Save));
                }
                next.set(CharMenu::Closed);
                return;
            }
            Choice::Buy(i) => {
                let stat = STATS_ORDER[i];
                let level = stat.level(&stats);
                if level < LEVEL_MAX && energy.0 >= upgrade_cost(level) {
                    energy.0 -= upgrade_cost(level);
                    *stat.level_mut(&mut stats) = level + 1;
                    if matches!(stat, Stat::Vitality) {
                        health.max = stats.max_hearts();
                        health.current = health.max; // the new heart comes in filled
                    }
                    write_progress(&mut save, &energy, &stats, &lost);
                }
            }
            Choice::Leave => {
                next.set(CharMenu::Closed);
                return;
            }
        }
    }

    // Refresh every live line.
    for (line, mut text, mut color) in &mut lines {
        match line {
            Line::Option(idx) => {
                let choice = options[*idx];
                text.0 = choice.label(&stats);
                let selected = *idx == cursor.0;
                *color = TextColor(option_color(choice, selected, &stats, &energy));
            }
            Line::Energy => {
                text.0 = format!("Energy: {}", energy.0);
                *color = TextColor(Color::srgb(0.6, 0.95, 0.7));
            }
            Line::Lost => {
                if lost.amount > 0 {
                    text.0 = format!("Lost {} energy in {}", lost.amount, lost.room);
                    *color = TextColor(Color::srgb(0.9, 0.45, 0.5));
                } else {
                    text.0.clear();
                }
            }
        }
    }
}

/// Colour for an option row: gold when picked, green when maxed, dimmed when a buy
/// is unaffordable, grey otherwise.
fn option_color(choice: Choice, selected: bool, stats: &Stats, energy: &Energy) -> Color {
    if let Choice::Buy(i) = choice {
        let level = STATS_ORDER[i].level(stats);
        let maxed = level >= LEVEL_MAX;
        let affordable = !maxed && energy.0 >= upgrade_cost(level);
        return match (selected, maxed, affordable) {
            (true, true, _) => Color::srgb(0.55, 0.85, 0.6),
            (true, _, true) => Color::srgb(1.0, 0.85, 0.3),
            (true, _, false) => Color::srgb(0.75, 0.55, 0.3),
            (false, true, _) => Color::srgb(0.45, 0.6, 0.5),
            (false, _, _) => Color::srgb(0.62, 0.64, 0.72),
        };
    }
    // Rest / Leave.
    if selected {
        Color::srgb(1.0, 0.85, 0.3)
    } else {
        Color::srgb(0.62, 0.64, 0.72)
    }
}
