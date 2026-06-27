//! Moving platforms — ridable kinematic tile groups.
//!
//! A room's [`Mover`](crate::world::Mover) data describes a rigid group of tiles whose
//! anchor travels a path of cells; [`spawn_mover`] turns one into a live [`Platform`]
//! (a parent entity that carries a child sprite per tile). Each frame [`move_platforms`]
//! advances them and republishes every tile as a [`PlatformBox`] in the
//! [`Platforms`] resource, which the player's movement resolves against (and rides — see
//! [`crate::player`] / [`crate::physics`]).

use bevy::prelude::*;

use crate::GameSet;
use crate::physics::{PlatformBox, Platforms, TILE};
use crate::world::{GameAssets, MapEntity, MoveMode, Mover};

/// A live moving platform. The entity's `Transform` is the anchor's world position; each
/// tile sits at `anchor + offsets[i]`. It eases between `points` (world anchor stops,
/// `points[0]` = home) at `speed` px/s, pausing `rest` s at each, cycling per `mode`.
#[derive(Component)]
pub struct Platform {
    offsets: Vec<Vec2>,
    points: Vec<Vec2>,
    mode: MoveMode,
    speed: f32,
    rest: f32,
    target: usize, // index in `points` we're moving toward
    dir: i32,      // travel direction along `points` (for ping-pong)
    resting: f32,  // seconds left paused at the current stop
    done: bool,    // a `Once` mover that has reached its last stop
    delta: Vec2,   // how far it moved this frame (for carrying riders)
}

pub struct MoversPlugin;

impl Plugin for MoversPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Platforms>().add_systems(
            Update,
            // Before the player so it rides this frame's motion, not last frame's.
            move_platforms
                .in_set(GameSet::Movement)
                .before(crate::player::movement),
        );
    }
}

/// Spawn one [`Mover`]'s platform: the anchor entity plus a child sprite per tile.
/// `height` is the room's row count, to flip authored rows (0 = top) into world space.
pub(crate) fn spawn_mover(
    commands: &mut Commands,
    assets: &GameAssets,
    mover: &Mover,
    height: i32,
) {
    let to_world = |(col, row): (i32, i32)| {
        Vec2::new(
            col as f32 * TILE + TILE / 2.0,
            (height - 1 - row) as f32 * TILE + TILE / 2.0,
        )
    };
    let home = to_world(mover.tiles[0]);
    let offsets: Vec<Vec2> = mover.tiles.iter().map(|&t| to_world(t) - home).collect();
    let mut points = vec![home];
    points.extend(mover.path.iter().map(|&p| to_world(p)));

    let platform = Platform {
        offsets: offsets.clone(),
        target: if points.len() >= 2 { 1 } else { 0 },
        points,
        mode: mover.mode,
        speed: mover.speed.max(0.0),
        rest: (mover.rest / 1000.0).max(0.0),
        dir: 1,
        resting: 0.0,
        done: false,
        delta: Vec2::ZERO,
    };

    commands
        .spawn((
            MapEntity,
            platform,
            Transform::from_xyz(home.x, home.y, 0.0),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            for off in &offsets {
                parent.spawn((
                    Sprite {
                        image: assets.tile.clone(),
                        custom_size: Some(Vec2::splat(TILE)),
                        ..default()
                    },
                    Transform::from_xyz(off.x, off.y, 0.0),
                ));
            }
        });
}

/// Advance each platform along its path and republish its tile boxes for collision.
fn move_platforms(
    time: Res<Time>,
    mut platforms: Query<(&mut Transform, &mut Platform)>,
    mut boxes: ResMut<Platforms>,
) {
    let dt = time.delta_secs();
    boxes.0.clear();
    for (mut transform, mut p) in &mut platforms {
        let prev = transform.translation.truncate();

        if p.points.len() >= 2 && !p.done {
            if p.resting > 0.0 {
                p.resting = (p.resting - dt).max(0.0);
            } else {
                let target = p.points[p.target];
                let cur = transform.translation.truncate();
                let to = target - cur;
                let dist = to.length();
                let step = p.speed * dt;
                if dist <= step || dist < 1e-3 {
                    transform.translation.x = target.x;
                    transform.translation.y = target.y;
                    p.resting = p.rest;
                    advance_target(&mut p);
                } else {
                    let mv = to / dist * step;
                    transform.translation.x += mv.x;
                    transform.translation.y += mv.y;
                }
            }
        }

        let now = transform.translation.truncate();
        p.delta = now - prev;
        for off in &p.offsets {
            boxes.0.push(PlatformBox {
                center: now + *off,
                half: Vec2::splat(TILE / 2.0),
                delta: p.delta,
            });
        }
    }
}

/// Pick the next stop after arriving, per the platform's [`MoveMode`].
fn advance_target(p: &mut Platform) {
    let n = p.points.len();
    match p.mode {
        MoveMode::Loop => p.target = (p.target + 1) % n,
        MoveMode::Once => {
            if p.target + 1 >= n {
                p.done = true;
            } else {
                p.target += 1;
            }
        }
        MoveMode::PingPong => {
            if p.dir > 0 && p.target + 1 >= n {
                p.dir = -1;
                p.target = p.target.saturating_sub(1);
            } else if p.dir < 0 && p.target == 0 {
                p.dir = 1;
                p.target = (n - 1).min(1);
            } else {
                p.target = (p.target as i32 + p.dir) as usize;
            }
        }
    }
}
