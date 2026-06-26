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

/// How long i-frames last after taking a hit.
const IFRAMES: f32 = 1.2;

/// Written when the player should take a hit (by hazards, pits, …).
#[derive(Message, Default)]
pub(crate) struct Hurt;

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
            .add_message::<Hurt>()
            .add_systems(OnEnter(GameState::Playing), spawn_hud)
            .add_systems(OnExit(GameState::Playing), despawn_hud)
            .add_systems(
                Update,
                (tick_invuln, apply_damage).chain().in_set(GameSet::Hazards),
            )
            .add_systems(Update, update_hud.run_if(in_state(GameState::Playing)));
    }
}

fn tick_invuln(time: Res<Time>, mut invuln: ResMut<Invuln>) {
    invuln.0 = (invuln.0 - time.delta_secs()).max(0.0);
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn apply_damage(
    mut hurts: MessageReader<Hurt>,
    mut health: ResMut<Health>,
    mut invuln: ResMut<Invuln>,
    mut armed: ResMut<TeleportArmed>,
    save: Res<Save>,
    respawn: Res<RespawnPoint>,
    mut load: MessageWriter<LoadMap>,
    mut player: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    // Drain the queue; one hit per i-frame window however many sources fired.
    let hit = hurts.read().count() > 0;
    if !hit || invuln.0 > 0.0 {
        return;
    }

    health.current -= 1;
    invuln.0 = IFRAMES;
    // Disarm teleporters: the room-entry respawn can be a portal pad, and we don't
    // want landing back on it to immediately teleport the player away again.
    armed.0 = false;

    if health.current > 0 {
        // Non-fatal: bounce back to where the player entered this room.
        if let Ok((mut transform, mut velocity)) = player.single_mut() {
            transform.translation.x = respawn.0.x;
            transform.translation.y = respawn.0.y;
            velocity.0 = Vec2::ZERO;
        }
    } else {
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
