//! Environmental dangers: ground spikes and falling rocks.
//!
//! Anything carrying a [`Hazard`] kills the player on contact. Death is instant
//! and forgiving — you respawn at the room's entry point ([`RespawnPoint`]),
//! Celeste-style, so precision sections stay fun. (Falling off the world is
//! handled by room transitions in [`crate::world`], not here.)

use bevy::prelude::*;

use crate::GameSet;
use crate::physics::{self, Solids, TILE};
use crate::player::{PLAYER_HALF, Player, Velocity};
use crate::world::MapEntity;

/// Deadly half-extents of a spike cell (a touch smaller than the tile, to be fair).
pub const SPIKE_HALF: Vec2 = Vec2::new(TILE * 0.4, TILE * 0.36);
const ROCK_HALF: Vec2 = Vec2::new(7.0, 7.0);
const ROCK_SIZE: f32 = 18.0;
const ROCK_GRAVITY: f32 = 1600.0;
const ROCK_MAX_FALL: f32 = 1100.0;
/// Rocks that fall below this y (just under the floor) are removed.
const DEATH_Y: f32 = -64.0;

/// Anything that kills the player on contact, with its half-extents.
#[derive(Component)]
pub struct Hazard {
    pub half: Vec2,
}

/// Periodically drops a [`Rock`].
#[derive(Component)]
pub struct RockSpawner {
    pub timer: Timer,
}

#[derive(Component)]
pub struct Rock {
    vy: f32,
}

/// Where the player returns after dying (the current map's entry spawn).
#[derive(Resource, Default)]
pub struct RespawnPoint(pub Vec2);

/// The falling-rock sprite, set by the world on load.
#[derive(Resource)]
pub struct RockSprite(pub Handle<Image>);

pub struct HazardPlugin;

impl Plugin for HazardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RespawnPoint>().add_systems(
            Update,
            (spawn_rocks, update_rocks, hazard_respawn)
                .chain()
                .in_set(GameSet::Hazards),
        );
    }
}

fn spawn_rocks(
    time: Res<Time>,
    mut commands: Commands,
    rock: Res<RockSprite>,
    mut spawners: Query<(&Transform, &mut RockSpawner)>,
) {
    for (transform, mut spawner) in &mut spawners {
        spawner.timer.tick(time.delta());
        if spawner.timer.just_finished() {
            commands.spawn((
                MapEntity,
                Rock { vy: 0.0 },
                Hazard { half: ROCK_HALF },
                Sprite {
                    image: rock.0.clone(),
                    custom_size: Some(Vec2::splat(ROCK_SIZE)),
                    ..default()
                },
                Transform::from_xyz(transform.translation.x, transform.translation.y, 2.0),
            ));
        }
    }
}

fn update_rocks(
    time: Res<Time>,
    solids: Res<Solids>,
    mut commands: Commands,
    mut rocks: Query<(Entity, &mut Transform, &mut Rock)>,
) {
    let dt = time.delta_secs();
    for (entity, mut transform, mut rock) in &mut rocks {
        rock.vy = (rock.vy - ROCK_GRAVITY * dt).max(-ROCK_MAX_FALL);
        let mut center = transform.translation.truncate();
        let (blocked, _) = physics::collide_y(&solids, &mut center, ROCK_HALF, rock.vy * dt);
        transform.translation.x = center.x;
        transform.translation.y = center.y;
        if blocked || transform.translation.y < DEATH_Y {
            commands.entity(entity).despawn();
        }
    }
}

fn hazard_respawn(
    respawn: Res<RespawnPoint>,
    hazards: Query<(&Transform, &Hazard), Without<Player>>,
    mut player: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    let Ok((mut player_tf, mut velocity)) = player.single_mut() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Falling off the world is handled by room transitions; here we only kill on
    // contact with a hazard (spikes, falling rocks).
    let mut died = false;
    for (hazard_tf, hazard) in &hazards {
        let delta = (hazard_tf.translation.truncate() - player_pos).abs();
        if delta.x < hazard.half.x + PLAYER_HALF.x && delta.y < hazard.half.y + PLAYER_HALF.y {
            died = true;
            break;
        }
    }

    if died {
        player_tf.translation.x = respawn.0.x;
        player_tf.translation.y = respawn.0.y;
        velocity.0 = Vec2::ZERO;
    }
}
