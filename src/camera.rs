//! A 2D camera that follows the player but stays bounded to the current room, and
//! **zooms in** to fill the screen when a room is smaller than the viewport. It
//! snaps on a room transition and glides otherwise.
//!
//! The projection is locked to a **fixed 960×540 logical viewport**
//! ([`ScalingMode::Fixed`]) so the fit/clamp maths below — and the HUD anchors that
//! share [`VIEW_HALF`] — stay correct at any window size. Resizing scales this canvas
//! to the window rather than revealing more of the world (which would let the view
//! spill outside the room). To keep that from **stretching** on a non-16:9 window,
//! [`letterbox`] confines rendering to the largest centred 16:9 rectangle that fits,
//! leaving black bars on the extra side.

use bevy::camera::{ScalingMode, Viewport};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::GameSet;
use crate::input::PlayerIntent;
use crate::menu::Paused;
use crate::player::{JumpState, Player, Velocity};
use crate::state::GameState;
use crate::world::RoomView;
use crate::worldmap::MapView;

/// Half the 960×540 viewport at scale 1, in world units (1 unit = 1 logical px).
const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);

/// How far the camera pans when looking up / crouching (world px), and how long Up/Down
/// must be held first (so a tap doesn't jolt the view).
const LOOK_DISTANCE: f32 = 130.0;
const LOOK_DELAY: f32 = 0.4;

#[derive(Component)]
pub(crate) struct GameCamera;

/// Eased vertical look-offset state for the up/down camera pan.
#[derive(Resource, Default)]
pub(crate) struct LookPan {
    /// Seconds Up/Down has been held (past [`LOOK_DELAY`] the pan engages).
    hold: f32,
    /// Current eased offset added to the camera's focus (clamped to the room).
    offset: f32,
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LookPan>()
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, follow.in_set(GameSet::Camera))
            // Keep the render area letterboxed to 16:9 at any window size/aspect.
            .add_systems(Update, letterbox)
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
    commands.spawn((
        Camera2d,
        // Always show exactly a 960×540 world rectangle (times `ortho.scale`),
        // independent of the window size; resizing just scales the canvas.
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: VIEW_HALF.x * 2.0,
                height: VIEW_HALF.y * 2.0,
            },
            ..OrthographicProjection::default_2d()
        }),
        GameCamera,
    ));
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

#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
pub(crate) fn follow(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    mut room_view: ResMut<RoomView>,
    mut look: ResMut<LookPan>,
    player: Query<(&Transform, &JumpState, &Velocity), (With<Player>, Without<GameCamera>)>,
    mut camera: Query<(&mut Transform, &mut Projection), With<GameCamera>>,
) {
    let Ok((player_tf, jump, velocity)) = player.single() else {
        return;
    };
    let Ok((mut camera_tf, mut projection)) = camera.single_mut() else {
        return;
    };
    let dt = time.delta_secs();

    let room = room_view.size;
    let scale = fit_scale(room);
    if let Projection::Orthographic(ortho) = &mut *projection {
        ortho.scale = scale;
    }

    // Look up / down: hold Up or Down while grounded and still to pan the view (clamped to
    // the room by `clamp_to_room`), after a short delay so taps don't jolt it.
    let look_dir = if jump.grounded() && velocity.0.x.abs() < 12.0 {
        f32::from(intent.up) - f32::from(intent.down)
    } else {
        0.0
    };
    look.hold = if look_dir != 0.0 { look.hold + dt } else { 0.0 };
    let target_off = if look.hold >= LOOK_DELAY {
        look_dir * LOOK_DISTANCE
    } else {
        0.0
    };
    look.offset += (target_off - look.offset) * (4.0 * dt).min(1.0);

    let focus = player_tf.translation.truncate() + Vec2::new(0.0, look.offset);
    let desired = clamp_to_room(focus, room, VIEW_HALF * scale);
    let current = camera_tf.translation.truncate();
    let next = if room_view.snap {
        room_view.snap = false;
        look.offset = 0.0; // don't carry a look-pan across a room change
        look.hold = 0.0;
        clamp_to_room(player_tf.translation.truncate(), room, VIEW_HALF * scale)
    } else {
        current.lerp(desired, (8.0 * dt).min(1.0))
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

/// The largest 16:9 rectangle that fits inside a `window` of physical pixels,
/// centred — returns `(top_left, size)`. Black bars fill whatever's left over.
fn letterbox_rect(window: UVec2) -> (UVec2, UVec2) {
    let target = VIEW_HALF.x / VIEW_HALF.y; // 16:9
    let (w, h) = if window.x as f32 / window.y as f32 > target {
        // Too wide: full height, bars on the left/right.
        (((window.y as f32) * target).round() as u32, window.y)
    } else {
        // Too tall (or exact): full width, bars on the top/bottom.
        (window.x, ((window.x as f32) / target).round() as u32)
    };
    let size = UVec2::new(w.min(window.x), h.min(window.y));
    let pos = (window - size) / 2;
    (pos, size)
}

/// Confine the camera's render target to a centred 16:9 viewport so the fixed
/// 960×540 projection fills it without stretching, whatever the window's aspect.
fn letterbox(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut camera: Query<&mut Camera, With<GameCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok(mut camera) = camera.single_mut() else {
        return;
    };
    let size = window.physical_size();
    if size.x == 0 || size.y == 0 {
        return; // minimised — leave the viewport be
    }
    let (position, view_size) = letterbox_rect(size);
    // Only write when it actually changes, to avoid per-frame change detection.
    let unchanged = camera
        .viewport
        .as_ref()
        .is_some_and(|v| v.physical_position == position && v.physical_size == view_size);
    if !unchanged {
        camera.viewport = Some(Viewport {
            physical_position: position,
            physical_size: view_size,
            ..default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aspect(size: UVec2) -> f32 {
        size.x as f32 / size.y as f32
    }

    #[test]
    fn exact_16_9_fills_the_window() {
        let (pos, size) = letterbox_rect(UVec2::new(1920, 1080));
        assert_eq!(pos, UVec2::ZERO);
        assert_eq!(size, UVec2::new(1920, 1080));
    }

    #[test]
    fn wide_window_gets_side_bars() {
        let (pos, size) = letterbox_rect(UVec2::new(2000, 1000));
        assert_eq!(size.y, 1000); // full height
        assert!(size.x < 2000); // narrower than the window
        assert!((aspect(size) - 16.0 / 9.0).abs() < 0.01);
        assert_eq!(pos.y, 0);
        assert_eq!(pos.x, (2000 - size.x) / 2); // centred horizontally
    }

    #[test]
    fn tall_window_gets_top_bottom_bars() {
        let (pos, size) = letterbox_rect(UVec2::new(1000, 1000));
        assert_eq!(size.x, 1000); // full width
        assert!(size.y < 1000); // shorter than the window
        assert!((aspect(size) - 16.0 / 9.0).abs() < 0.01);
        assert_eq!(pos.x, 0);
        assert_eq!(pos.y, (1000 - size.y) / 2); // centred vertically
    }
}
