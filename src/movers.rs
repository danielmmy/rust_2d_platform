//! Moving platforms — a generic "carry these entities along a path" system.
//!
//! A room's [`Mover`](crate::world::Mover) names a group of cells; [`crate::world`] spawns
//! whatever each cell holds (a solid tile, a spike, a bench, …) and hands those entities
//! to [`spawn_mover`] as the platform's **parts**. The mover doesn't create art of its
//! own — it just *moves what's there*, so the same data drives a ridable platform, a
//! sweeping spike, a roving bench, and so on. Each frame [`move_platforms`] advances the
//! anchor, writes every part's world `Transform`, and republishes the **solid** parts as
//! [`PlatformBox`]es in [`Platforms`] so the player collides with and rides them
//! (see [`crate::player`] / [`crate::physics`]).

use bevy::prelude::*;

use crate::GameSet;
use crate::physics::{PlatformBox, Platforms, TILE};
use crate::world::{MapEntity, MoveMode, Mover};

/// One entity carried by a platform: its offset from the anchor and whether it's solid
/// (only solid parts become ride/collision boxes).
struct MoverPart {
    entity: Entity,
    offset: Vec2,
    solid: bool,
}

/// A live moving platform. `anchor` is the group's current world position; it eases
/// between `points` (anchor stops, `points[0]` = home) at `speed` px/s, pausing `rest` s
/// at each, cycling per `mode`, and drives each part's `Transform` to `anchor + offset`.
#[derive(Component)]
pub struct Platform {
    anchor: Vec2,
    points: Vec<Vec2>,
    mode: MoveMode,
    speed: f32,
    rest: f32,
    target: usize, // index in `points` we're moving toward
    dir: i32,      // travel direction along `points` (for ping-pong)
    resting: f32,  // seconds left paused at the current stop
    done: bool,    // a `Once` mover that has reached its last stop
    delta: Vec2,   // how far it moved this frame (for carrying riders)
    parts: Vec<MoverPart>,
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

/// Create a platform from a [`Mover`] and the entities picked up at its cells (each as
/// `(entity, offset_from_home, is_solid)`). `height` is the room's row count, to flip
/// authored rows (0 = top) into world space.
pub(crate) fn spawn_mover(
    commands: &mut Commands,
    mover: &Mover,
    height: i32,
    home: Vec2,
    parts: Vec<(Entity, Vec2, bool)>,
) {
    let to_world = |(col, row): (i32, i32)| {
        Vec2::new(
            col as f32 * TILE + TILE / 2.0,
            (height - 1 - row) as f32 * TILE + TILE / 2.0,
        )
    };
    let mut points = vec![home];
    points.extend(mover.path.iter().map(|&p| to_world(p)));

    let parts = parts
        .into_iter()
        .map(|(entity, offset, solid)| MoverPart {
            entity,
            offset,
            solid,
        })
        .collect();

    commands.spawn((
        MapEntity,
        Platform {
            anchor: home,
            target: if points.len() >= 2 { 1 } else { 0 },
            points,
            mode: mover.mode,
            speed: mover.speed.max(0.0),
            rest: (mover.rest / 1000.0).max(0.0),
            dir: 1,
            resting: 0.0,
            done: false,
            delta: Vec2::ZERO,
            parts,
        },
    ));
}

/// Advance each platform, write its parts' world transforms, and republish solid boxes.
fn move_platforms(
    time: Res<Time>,
    mut platforms: Query<&mut Platform>,
    mut transforms: Query<&mut Transform>,
    mut boxes: ResMut<Platforms>,
) {
    let dt = time.delta_secs();
    boxes.0.clear();
    for mut p in &mut platforms {
        let prev = p.anchor;

        if p.points.len() >= 2 && !p.done {
            if p.resting > 0.0 {
                p.resting = (p.resting - dt).max(0.0);
            } else {
                let target = p.points[p.target];
                let to = target - p.anchor;
                let dist = to.length();
                let step = p.speed * dt;
                if dist <= step || dist < 1e-3 {
                    p.anchor = target;
                    p.resting = p.rest;
                    advance_target(&mut p);
                } else {
                    p.anchor += to / dist * step;
                }
            }
        }

        p.delta = p.anchor - prev;
        let (anchor, delta) = (p.anchor, p.delta);
        for part in &p.parts {
            if let Ok(mut tf) = transforms.get_mut(part.entity) {
                tf.translation.x = anchor.x + part.offset.x;
                tf.translation.y = anchor.y + part.offset.y;
            }
            if part.solid {
                boxes.0.push(PlatformBox {
                    center: anchor + part.offset,
                    half: Vec2::splat(TILE / 2.0),
                    delta,
                });
            }
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
