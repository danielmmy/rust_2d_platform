//! Health, damage, invulnerability frames, the health-bar HUD, and death → last bench.
//!
//! The player has [`Health`] (three points by default; Vitality raises the max).
//! Hazards and pits write a [`Hurt`] message; [`apply_damage`] spends a point, grants
//! brief i-frames, and either respawns at the room entrance (a non-fatal hit) or — on
//! the last point — refills and sends the player to the last bench they rested at (see
//! [`crate::save`]). The HUD is a single continuous bar pinned to the viewport, its
//! fill width and colour (green → red) tracking the health fraction.

use bevy::prelude::*;

use crate::GameSet;
use crate::audio::{PlaySfx, Sfx};
use crate::hazards::RespawnPoint;
use crate::player::{Player, Velocity};
use crate::save::Save;
use crate::state::GameState;
use crate::stats::{Died, Stats};
use crate::world::{CurrentRoom, Entry, LoadMap, START_MAP, TeleportArmed};

/// The player's hearts.
#[derive(Resource)]
pub struct Health {
    pub current: i32,
    pub max: i32,
}

impl Default for Health {
    fn default() -> Self {
        Self { current: 3, max: 3 }
    }
}

/// Seconds of invulnerability remaining after a hit (no further damage taken).
#[derive(Resource, Default)]
pub struct Invuln(pub f32);

/// Seconds of stun remaining: while > 0 the player can't steer (they're being
/// knocked back). Read by [`crate::player`] movement.
#[derive(Resource, Default)]
pub struct Stun(pub f32);

/// How long i-frames last after taking a hit.
const IFRAMES: f32 = 1.2;
/// How long the player is stunned (no control) during a knockback.
const STUN_TIME: f32 = 0.22;
/// Knockback velocity applied on a hit (away from the source, plus a little up).
const KNOCKBACK_X: f32 = 320.0;
const KNOCKBACK_Y: f32 = 240.0;

/// Written when the player should take a hit.
#[derive(Message, Clone, Copy)]
pub(crate) enum Hurt {
    /// Hit by something at this world position — knock the player away from it.
    From(Vec2),
    /// Fell out of the world with no ground to land on — respawn at the room entry.
    Pit,
}

/// The dark backing of the health bar (shows through where health is missing).
#[derive(Component)]
struct HealthBarBg;
/// The coloured fill of the health bar; its width tracks the health fraction and its
/// hue slides green → red as that fraction drops.
#[derive(Component)]
struct HealthBarFill;

// Health-bar layout, in viewport pixels (scaled with the camera so it stays put). A
// single continuous bar (rather than discrete pips) reads cleanly however high
// Vitality pushes the max — health is always shown as a fraction of the same bar.
const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);
const BAR_SIZE: Vec2 = Vec2::new(180.0, 16.0);
/// Offset of the bar's left-centre from the viewport's top-left corner.
const BAR_INSET: Vec2 = Vec2::new(20.0, 24.0);
const BAR_BG: Color = Color::srgb(0.12, 0.12, 0.16);

pub struct HealthPlugin;

impl Plugin for HealthPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Health>()
            .init_resource::<Invuln>()
            .init_resource::<Stun>()
            .add_message::<Hurt>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnExit(GameState::Playing), despawn_hud)
            .add_systems(
                Update,
                (tick_timers, apply_damage).chain().in_set(GameSet::Hazards),
            )
            .add_systems(Update, update_hud.run_if(in_state(GameState::Playing)));
    }
}

fn tick_timers(time: Res<Time>, mut invuln: ResMut<Invuln>, mut stun: ResMut<Stun>) {
    let dt = time.delta_secs();
    invuln.0 = (invuln.0 - dt).max(0.0);
    stun.0 = (stun.0 - dt).max(0.0);
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn apply_damage(
    mut hurts: MessageReader<Hurt>,
    mut health: ResMut<Health>,
    mut invuln: ResMut<Invuln>,
    mut stun: ResMut<Stun>,
    mut armed: ResMut<TeleportArmed>,
    stats: Res<Stats>,
    save: Res<Save>,
    current: Res<CurrentRoom>,
    respawn: Res<RespawnPoint>,
    mut load: MessageWriter<LoadMap>,
    mut died: MessageWriter<Died>,
    mut sfx: MessageWriter<PlaySfx>,
    mut player: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    // Drain the queue (so none go stale); take the first hit's source. One hit per
    // i-frame window, however many sources fired.
    let sources: Vec<Hurt> = hurts.read().copied().collect();
    let Some(&hurt) = sources.first() else {
        return;
    };
    if invuln.0 > 0.0 {
        return;
    }

    health.current -= 1;
    sfx.write(PlaySfx(Sfx::Hurt));
    invuln.0 = IFRAMES;
    // Disarm teleporters: a respawn/knockback can leave the player on a portal pad,
    // and we don't want it to immediately fire and teleport them away.
    armed.0 = false;

    let Ok((mut transform, mut velocity)) = player.single_mut() else {
        return;
    };

    if health.current <= 0 {
        // Out of hearts: drop a bloodstain here (the death spot — but for a pit, the
        // reachable room entry), then refill and return to the last bench.
        let death_pos = match hurt {
            Hurt::Pit => respawn.0,
            Hurt::From(_) => transform.translation.truncate(),
        };
        died.write(Died {
            pos: death_pos,
            room: current.name.clone(),
        });
        health.current = health.max;
        let (map, entry) = if save.has_bench() {
            (
                save.bench_room.clone(),
                Entry::Bench(save.bench_col, save.bench_row),
            )
        } else {
            (START_MAP.to_string(), Entry::Start)
        };
        load.write(LoadMap { map, entry });
        return;
    }

    match hurt {
        // Knock the player back away from what hit them, and stun briefly so the
        // hit registers regardless of input. Poise shortens the stagger.
        Hurt::From(source) => {
            let dir = if transform.translation.x >= source.x {
                1.0
            } else {
                -1.0
            };
            velocity.0 = Vec2::new(dir * KNOCKBACK_X, KNOCKBACK_Y);
            stun.0 = STUN_TIME * stats.stun_scale();
        }
        // Fell into a bottomless pit — nowhere to knock back to, so respawn at the
        // room's entry.
        Hurt::Pit => {
            transform.translation.x = respawn.0.x;
            transform.translation.y = respawn.0.y;
            velocity.0 = Vec2::ZERO;
        }
    }
}

fn spawn_hud(mut commands: Commands, existing: Query<(), With<HealthBarBg>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        HealthBarBg,
        Sprite {
            color: BAR_BG,
            custom_size: Some(BAR_SIZE),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 50.0),
    ));
    commands.spawn((
        HealthBarFill,
        Sprite {
            custom_size: Some(BAR_SIZE),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 51.0),
    ));
}

#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn despawn_hud(
    mut commands: Commands,
    bars: Query<Entity, Or<(With<HealthBarBg>, With<HealthBarFill>)>>,
) {
    for entity in &bars {
        commands.entity(entity).despawn();
    }
}

/// Pin the health bar to the top-left of the viewport (scaled with the camera zoom so
/// it keeps a constant on-screen size). The fill's width is the health fraction and
/// its colour slides from green (full) through yellow to red (low).
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn update_hud(
    health: Res<Health>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    mut bg: Query<&mut Transform, (With<HealthBarBg>, Without<Camera2d>)>,
    mut fill: Query<
        (&mut Transform, &mut Sprite),
        (With<HealthBarFill>, Without<HealthBarBg>, Without<Camera2d>),
    >,
) {
    let Ok((camera_tf, projection)) = camera.single() else {
        return;
    };
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };
    let top_left = camera_tf.translation.truncate() + Vec2::new(-VIEW_HALF.x, VIEW_HALF.y) * scale;
    // The bar's left-centre, in world space.
    let left = top_left + Vec2::new(BAR_INSET.x, -BAR_INSET.y) * scale;

    // Sprites are centre-anchored, so a sprite of (world) width `w` whose left edge
    // sits at `left.x` has its centre at `left.x + w/2`.
    if let Ok(mut bg_tf) = bg.single_mut() {
        bg_tf.translation.x = left.x + BAR_SIZE.x * 0.5 * scale;
        bg_tf.translation.y = left.y;
        bg_tf.scale = Vec3::splat(scale);
    }

    if let Ok((mut fill_tf, mut sprite)) = fill.single_mut() {
        let frac = if health.max > 0 {
            (health.current as f32 / health.max as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let width = BAR_SIZE.x * frac;
        sprite.custom_size = Some(Vec2::new(width, BAR_SIZE.y));
        // Hue 120° (green) at full health → 0° (red) when empty.
        sprite.color = Color::hsl(120.0 * frac, 0.85, 0.5);
        fill_tf.translation.x = left.x + width * 0.5 * scale;
        fill_tf.translation.y = left.y;
        fill_tf.scale = Vec3::splat(scale);
    }
}
