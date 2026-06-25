//! A 2D camera that follows the player but stays bounded to the current room, and
//! **zooms in** to fill the screen when a room is smaller than the viewport. It
//! snaps on a room transition and glides otherwise.

use bevy::prelude::*;

use crate::GameSet;
use crate::menu::Paused;
use crate::player::Player;
use crate::state::GameState;
use crate::world::RoomView;
use crate::worldmap::MapView;

/// Half the 960×540 viewport at scale 1, in world units (1 unit = 1 logical px).
const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);

#[derive(Component)]
struct GameCamera;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, follow.in_set(GameSet::Camera))
            // Overlays (menus / world map / builder) are drawn at 1:1, so un-zoom
            // whenever we're not in active gameplay.
            .add_systems(
                Update,
                reset_zoom.run_if(not(in_state(GameState::Playing)
                    .and_then(in_state(MapView::Closed))
                    .and_then(in_state(Paused::Running)))),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((Camera2d, GameCamera));
}

/// Zoom factor that makes a small room fill the viewport; never below 1 (large
/// rooms keep scale 1 and scroll instead).
fn fit_scale(room: Vec2) -> f32 {
    if room.x <= 0.0 || room.y <= 0.0 {
        return 1.0;
    }
    (room.x / (VIEW_HALF.x * 2.0))
        .max(room.y / (VIEW_HALF.y * 2.0))
        .min(1.0)
}

/// Clamp `target` so the (scaled) view never shows outside a room; if the room is
/// smaller than the view on an axis, centre it there.
fn clamp_to_room(target: Vec2, room: Vec2, half: Vec2) -> Vec2 {
    let clamp_axis = |t: f32, h: f32, size: f32| {
        if size >= h * 2.0 {
            t.clamp(h, size - h)
        } else {
            size / 2.0
        }
    };
    Vec2::new(
        clamp_axis(target.x, half.x, room.x),
        clamp_axis(target.y, half.y, room.y),
    )
}

fn follow(
    time: Res<Time>,
    mut room_view: ResMut<RoomView>,
    player: Query<&Transform, (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let Ok((mut camera_tf, mut projection)) = camera.single_mut() else {
        return;
    };

    let room = room_view.size;
    let scale = fit_scale(room);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = scale;
    }

    let desired = clamp_to_room(player_tf.translation.truncate(), room, VIEW_HALF * scale);
    let current = camera_tf.translation.truncate();
    let next = if room_view.snap {
        room_view.snap = false;
        desired
    } else {
        current.lerp(desired, (8.0 * time.delta_secs()).min(1.0))
    };
    camera_tf.translation.x = next.x;
    camera_tf.translation.y = next.y;
}

fn reset_zoom(mut camera: Query<&mut Projection, With<GameCamera>>) {
    if let Ok(mut projection) = camera.single_mut()
        && let Projection::Orthographic(ortho) = &mut *projection
    {
        ortho.scale = 1.0;
    }
}
