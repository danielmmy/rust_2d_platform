//! Unified input: keyboard **and** gamepad collapse into one intent resource,
//! so the rest of the game never cares which device drove it.

use bevy::prelude::*;

use crate::GameSet;

/// What the player is asking for this frame.
#[derive(Resource, Default)]
pub struct PlayerIntent {
    /// Horizontal axis in `[-1, 1]`.
    pub move_x: f32,
    /// Jump was pressed this frame (edge) — feeds the jump buffer.
    pub jump_pressed: bool,
    /// Jump is currently held — feeds variable jump height.
    pub jump_held: bool,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerIntent>()
            .add_systems(Update, gather.in_set(GameSet::Input));
    }
}

const JUMP_KEYS: [KeyCode; 4] = [
    KeyCode::Space,
    KeyCode::KeyW,
    KeyCode::ArrowUp,
    KeyCode::KeyZ,
];

fn gather(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut intent: ResMut<PlayerIntent>,
) {
    let mut move_x = 0.0;
    if keys.pressed(KeyCode::ArrowLeft) || keys.pressed(KeyCode::KeyA) {
        move_x -= 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) || keys.pressed(KeyCode::KeyD) {
        move_x += 1.0;
    }
    let mut jump_pressed = keys.any_just_pressed(JUMP_KEYS);
    let mut jump_held = keys.any_pressed(JUMP_KEYS);

    for gamepad in &gamepads {
        let stick = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        if stick.abs() > 0.3 {
            move_x += stick;
        }
        if gamepad.pressed(GamepadButton::DPadLeft) {
            move_x -= 1.0;
        }
        if gamepad.pressed(GamepadButton::DPadRight) {
            move_x += 1.0;
        }
        if gamepad.just_pressed(GamepadButton::South) {
            jump_pressed = true;
        }
        if gamepad.pressed(GamepadButton::South) {
            jump_held = true;
        }
    }

    intent.move_x = move_x.clamp(-1.0, 1.0);
    intent.jump_pressed = jump_pressed;
    intent.jump_held = jump_held;
}
