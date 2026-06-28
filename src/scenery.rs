//! Parallax scenery — layered, looping backgrounds that make rooms feel alive.
//!
//! A room names a **scenery set** (e.g. `"forest_meadow"`); [`spawn_scenery`] lays out
//! its layers as a small pool of tile sprites, and [`parallax`] repositions them against
//! the camera each frame: far layers drift slowly, nearer ones more, in both axes. Layers
//! **wrap horizontally** (any room width loops) and use **vertical parallax** so the
//! backdrop scrolls down as you climb rather than riding up with the camera. All layers
//! sit **behind** gameplay so nothing covers the player. Each set ships four tileable
//! layers (`far`/`mid`/`near`/`fg`) in `assets/scenery/` (see `tools/gen_scenery.py`).

use bevy::prelude::*;

use crate::GameSet;
use crate::camera::{GameCamera, follow};
use crate::world::{GameAssets, MapEntity};

/// Tileable layer size (must match `tools/gen_scenery.py`).
const SCENERY_W: f32 = 480.0;
const SCENERY_H: f32 = 560.0;
/// Half the logical viewport width (matches the camera's fixed projection).
const VIEW_HALF_W: f32 = 480.0;
/// Tiles per layer — enough to cover the view plus a wrap buffer.
const POOL: i32 = 5;

/// The layers every set provides: `(file suffix, horizontal parallax, vertical parallax,
/// z depth)`. Higher factor = moves more with the camera (closer). All sit **behind**
/// gameplay (negative z) so nothing ever covers the player. The far sky has no vertical
/// parallax, so it always fills the view; the nearer layers scroll vertically and may
/// leave the top open — that just reveals the sky behind them, Silksong-style.
const LAYERS: [(&str, f32, f32, f32); 4] = [
    ("far", 0.10, 0.0, -40.0),
    ("mid", 0.30, 0.12, -30.0),
    ("near", 0.55, 0.25, -20.0),
    ("fg", 0.78, 0.38, -10.0),
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
}

pub struct SceneryPlugin;

impl Plugin for SceneryPlugin {
    fn build(&self, app: &mut App) {
        // After the camera settles each frame, so tiles track its final position.
        app.add_systems(Update, parallax.in_set(GameSet::Camera).after(follow));
    }
}

/// Spawn the pooled tile sprites for a room's scenery `set` (no-op if empty/unknown).
/// Tagged [`MapEntity`] so they clear on the next room load.
pub(crate) fn spawn_scenery(commands: &mut Commands, assets: &GameAssets, set: &str) {
    if set.is_empty() {
        return;
    }
    for (suffix, hfactor, vfactor, z) in LAYERS {
        let key = format!("{set}_{suffix}.png");
        let Some(handle) = assets.scenery.get(&key) else {
            continue;
        };
        for slot in 0..POOL {
            commands.spawn((
                MapEntity,
                ParallaxTile {
                    hfactor,
                    vfactor,
                    span: SCENERY_W,
                    slot,
                },
                Sprite {
                    image: handle.clone(),
                    custom_size: Some(Vec2::new(SCENERY_W, SCENERY_H)),
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, z),
            ));
        }
    }
}

/// Slide every layer tile by its parallax factor: horizontally it wraps so the layer
/// loops across any room width; vertically it stays anchored in the world (smaller
/// factor = drifts less), so the backdrop scrolls down as you climb instead of riding up
/// with the camera. The far sky (vfactor 0) keeps filling behind any gaps the nearer,
/// transparent-topped layers leave.
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
        tf.translation.y = cam.y * (1.0 - tile.vfactor);
    }
}
