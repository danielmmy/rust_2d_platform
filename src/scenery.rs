//! Parallax scenery — layered, looping backdrops that make rooms feel alive.
//!
//! A room picks a **set per layer** ([`Scenery`]), so backdrops mix-and-match (a desert
//! sky over forest hills, …). [`spawn_scenery`] lays each layer out as a small pool of
//! tile sprites and [`parallax`] repositions them against the camera every frame:
//!
//! - **far / mid / near** are *background* (behind everything): they wrap horizontally and
//!   use **vertical parallax** so the backdrop scrolls down as you climb instead of riding
//!   up with the camera. The far sky stays camera-locked so it always fills behind.
//! - **fg** is a real *foreground* drawn **in front** of the player but **anchored to the
//!   ground**, so its sparse tufts sit at your feet and scroll off as you rise — never
//!   covering the player.
//!
//! Each set ships four tileable layers in `assets/scenery/<set>/` (`tools/gen_scenery.py`).

use bevy::prelude::*;

use crate::GameSet;
use crate::camera::{GameCamera, follow};
use crate::world::{GameAssets, MapEntity, Scenery};

const FAR_W: f32 = 1440.0; // wide sky → never visibly repeats across a room
const NEAR_W: f32 = 960.0; // mid / near / fg width (>= the viewport)
const SCENERY_H: f32 = 560.0;
const VIEW_HALF_W: f32 = 480.0;
/// World Y the foreground centres on, so its image bottom sits at the room floor (y≈0).
const GROUND_ANCHOR: f32 = SCENERY_H * 0.5;

/// One layer's runtime config: which [`Scenery`] field, file suffix, horizontal/vertical
/// parallax, z depth, tile width, and whether it's the ground-anchored foreground.
struct LayerSpec {
    suffix: &'static str,
    hfactor: f32,
    vfactor: f32,
    z: f32,
    width: f32,
    ground: bool,
}

const LAYERS: [LayerSpec; 4] = [
    LayerSpec {
        suffix: "far",
        hfactor: 0.10,
        vfactor: 0.0,
        z: -40.0,
        width: FAR_W,
        ground: false,
    },
    LayerSpec {
        suffix: "mid",
        hfactor: 0.30,
        vfactor: 0.14,
        z: -30.0,
        width: NEAR_W,
        ground: false,
    },
    LayerSpec {
        suffix: "near",
        hfactor: 0.55,
        vfactor: 0.28,
        z: -20.0,
        width: NEAR_W,
        ground: false,
    },
    LayerSpec {
        suffix: "fg",
        hfactor: 1.10,
        vfactor: 0.0,
        z: 30.0,
        width: NEAR_W,
        ground: true,
    },
];

/// The 12 shipped scenery sets (also the editor's cycle order).
pub(crate) const SETS: [&str; 12] = [
    "forest_meadow",
    "deep_caves",
    "snowy_mountains",
    "sandy_beach",
    "desolate_desert",
    "mushroom_hollow",
    "volcanic_depths",
    "sunset_cliffs",
    "crystal_grotto",
    "autumn_woods",
    "misty_swamp",
    "starry_void",
];

/// One tile of a parallax layer. `slot` is its index in the layer's wrap pool.
#[derive(Component)]
struct ParallaxTile {
    hfactor: f32,
    vfactor: f32,
    span: f32,
    slot: i32,
    ground: bool,
}

pub struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        // After the camera settles each frame, so tiles track its final position.
        app.add_systems(Update, parallax.in_set(GameSet::Camera).after(follow));
    }
}

/// Spawn the pooled tile sprites for a room's [`Scenery`]. Each layer is sourced from its
/// own set (`""` = skip), so layers mix freely. Tagged [`MapEntity`] to clear on reload.
pub(crate) fn spawn_scenery(commands: &mut Commands, assets: &GameAssets, scenery: &Scenery) {
    for spec in &LAYERS {
        let set = match spec.suffix {
            "far" => &scenery.far,
            "mid" => &scenery.mid,
            "near" => &scenery.near,
            _ => &scenery.fg,
        };
        if set.is_empty() {
            continue;
        }
        let key = format!("{set}/{}.png", spec.suffix);
        let Some(handle) = assets.scenery.get(&key) else {
            continue;
        };
        let pool = (VIEW_HALF_W * 2.0 / spec.width).ceil() as i32 + 2;
        for slot in 0..pool {
            commands.spawn((
                MapEntity,
                ParallaxTile {
                    hfactor: spec.hfactor,
                    vfactor: spec.vfactor,
                    span: spec.width,
                    slot,
                    ground: spec.ground,
                },
                Sprite {
                    image: handle.clone(),
                    custom_size: Some(Vec2::new(spec.width, SCENERY_H)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, spec.z),
            ));
        }
    }
}

/// Reposition every layer tile: wrap horizontally to loop across any room width, and set
/// the vertical position — a ground-anchored foreground stays at the world floor; the
/// background drifts with vertical parallax (far locked, nearer layers more).
fn parallax(
    camera: Query<(&Transform, &Projection), With<GameCamera>>,
    mut tiles: Query<(&ParallaxTile, &mut Transform), Without<GameCamera>>,
) {
    let Ok((cam_tf, projection)) = camera.single() else {
        return;
    };
    let cam = cam_tf.translation.truncate();
    let scale = match projection {
        Projection::Orthographic(o) => o.scale,
        _ => 1.0,
    };
    let half_w = VIEW_HALF_W * scale;
    for (tile, mut tf) in &mut tiles {
        let base = cam.x * (1.0 - tile.hfactor); // unwrapped x of tile index 0
        let first = ((cam.x - half_w - base) / tile.span).floor() as i32;
        tf.translation.x = base + (first + tile.slot) as f32 * tile.span;
        tf.translation.y = if tile.ground {
            GROUND_ANCHOR
        } else {
            cam.y * (1.0 - tile.vfactor)
        };
    }
}
