//! Hearts, damage, invulnerability frames, the heart HUD, and death → last bench.
//!
//! The player has [`Health`] (three hearts by default). Hazards and pits write a
//! [`Hurt`] message; [`apply_damage`] spends a heart, grants brief i-frames, and
//! either respawns at the room entrance (a non-fatal hit) or — on the last heart —
//! refills and sends the player to the last bench they rested at (see
//! [`crate::save`]). The HUD is a row of heart sprites pinned to the viewport.

use bevy::prelude::*;

use crate::GameSet;
use crate::hazards::RespawnPoint;
use crate::player::{Player, Velocity};
use crate::save::Save;
use crate::state::GameState;
use crate::world::{Entry, LoadMap, START_MAP, TeleportArmed};

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

/// One heart in the HUD, by index from the left.
#[derive(Component)]
struct HeartIcon(i32);

// Heart HUD layout, in viewport pixels (scaled with the camera so it stays put).
const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);
const HEART_SIZE: f32 = 24.0;
const HEART_FULL: Color = Color::srgb(0.9, 0.2, 0.3);
const HEART_EMPTY: Color = Color::srgb(0.28, 0.12, 0.14);

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
    save: Res<Save>,
    respawn: Res<RespawnPoint>,
    mut load: MessageWriter<LoadMap>,
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
    invuln.0 = IFRAMES;
    // Disarm teleporters: a respawn/knockback can leave the player on a portal pad,
    // and we don't want it to immediately fire and teleport them away.
    armed.0 = false;

    let Ok((mut transform, mut velocity)) = player.single_mut() else {
        return;
    };

    if health.current <= 0 {
        // Out of hearts: refill and return to the last bench (else the start room).
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
        // hit registers regardless of input.
        Hurt::From(source) => {
            let dir = if transform.translation.x >= source.x {
                1.0
            } else {
                -1.0
            };
            velocity.0 = Vec2::new(dir * KNOCKBACK_X, KNOCKBACK_Y);
            stun.0 = STUN_TIME;
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

fn spawn_hud(mut commands: Commands, health: Res<Health>, existing: Query<(), With<HeartIcon>>) {
    if !existing.is_empty() {
        return;
    }
    for i in 0..health.max {
        commands.spawn((
            HeartIcon(i),
            Sprite {
                color: HEART_FULL,
                custom_size: Some(Vec2::splat(HEART_SIZE)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 50.0),
        ));
    }
}

fn despawn_hud(mut commands: Commands, hearts: Query<Entity, With<HeartIcon>>) {
    for entity in &hearts {
        commands.entity(entity).despawn();
    }
}

/// Pin the hearts to the top-left of the viewport (scaled with the camera zoom so
/// they keep a constant on-screen size) and colour them by current health.
fn update_hud(
    health: Res<Health>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    mut hearts: Query<(&HeartIcon, &mut Transform, &mut Sprite), Without<Camera2d>>,
) {
    let Ok((camera_tf, projection)) = camera.single() else {
        return;
    };
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };
    let top_left = camera_tf.translation.truncate() + Vec2::new(-VIEW_HALF.x, VIEW_HALF.y) * scale;

    for (icon, mut transform, mut sprite) in &mut hearts {
        let pos = top_left + Vec2::new(28.0 + icon.0 as f32 * 32.0, -28.0) * scale;
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        transform.scale = Vec3::splat(scale);
        sprite.color = if icon.0 < health.current {
            HEART_FULL
        } else {
            HEART_EMPTY
        };
    }
}
