//! Environmental dangers: ground spikes and falling rocks.
//!
//! Anything carrying a [`Hazard`] hurts the player on contact, emitting a
//! [`Hurt`](crate::health) so the health system spends a heart and respawns at the
//! room's entry point ([`RespawnPoint`]) — or, on the last heart, at the last
//! bench. (Falling off the world is handled by room transitions in
//! [`crate::world`], which also reports a hit.)

use bevy::prelude::*;

use crate::GameSet;
use crate::health::{Hurt, Invuln};
use crate::physics::{self, Solids, TILE};
use crate::player::{PLAYER_HALF, Player};
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
            (spawn_rocks, update_rocks, hazard_contact)
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

fn hazard_contact(
    invuln: Res<Invuln>,
    grace: Res<crate::combat::PogoGrace>,
    hazards: Query<(&Transform, &Hazard), Without<Player>>,
    player: Query<&Transform, With<Player>>,
    mut hurt: MessageWriter<Hurt>,
) {
    // Skip while invulnerable (so one touch costs a single heart) or during the brief
    // grace after a pogo (so bouncing across spikes is safe).
    if invuln.0 > 0.0 || grace.0 > 0.0 {
        return;
    }
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Falling off the world is handled by room transitions; here we only hurt on
    // contact with a hazard (spikes, falling rocks, enemies) — knocking the player
    // away from it.
    let touched = hazards.iter().find(|(hazard_tf, hazard)| {
        let delta = (hazard_tf.translation.truncate() - player_pos).abs();
        delta.x < hazard.half.x + PLAYER_HALF.x && delta.y < hazard.half.y + PLAYER_HALF.y
    });
    if let Some((hazard_tf, _)) = touched {
        hurt.write(Hurt::From(hazard_tf.translation.truncate()));
    }
}
