//! Unified input: keyboard **and** gamepad collapse into one intent resource,
//! so the rest of the game never cares which device drove it. A [`LastInput`] resource
//! tracks which device was used most recently, so on-screen hints can show the matching
//! labels (keyboard keys, or PlayStation-style **Cross/Circle/Square/Triangle/L1/R1**).

use bevy::prelude::*;

use crate::GameSet;

/// What the player is asking for this frame.
#[derive(Resource, Default)]
pub struct PlayerIntent {
    /// Horizontal axis in `[-1, 1]`.
    pub move_x: f32,
    /// Up is held — looks up when grounded.
    pub up: bool,
    /// Down is held — crouches when grounded; aims a **pogo** down-slash when airborne.
    pub down: bool,
    /// Down was pressed this frame (edge) — starts a **slide** when running/dashing.
    pub down_pressed: bool,
    /// Jump was pressed this frame (edge) — feeds the jump buffer.
    pub jump_pressed: bool,
    /// Jump is currently held — feeds variable jump height.
    pub jump_held: bool,
    /// Interact was pressed this frame (edge) — e.g. resting at a bench.
    pub interact: bool,
    /// Attack was pressed this frame (edge) — swings the sword.
    pub attack_pressed: bool,
    /// Dash was pressed this frame (edge) — a quick horizontal burst (if unlocked).
    pub dash_pressed: bool,
    /// Dash is currently held — keep holding after a dash to **run** (sustained sprint).
    pub dash_held: bool,
}

/// The most recently used input device, so on-screen hints can show the right labels.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
pub enum LastInput {
    #[default]
    Keyboard,
    Gamepad,
}

impl LastInput {
    fn pad(self) -> bool {
        matches!(self, LastInput::Gamepad)
    }
    /// Confirm / choose — `Enter` or PlayStation **Cross**.
    pub fn confirm(self) -> &'static str {
        if self.pad() { "Cross" } else { "Enter" }
    }
    /// Back / cancel / close — `Esc` or **Circle**.
    pub fn cancel(self) -> &'static str {
        if self.pad() { "Circle" } else { "Esc" }
    }
    /// Up/Down menu navigation.
    pub fn updown(self) -> &'static str {
        if self.pad() { "D-Pad" } else { "Up/Down" }
    }
    /// Interact prompt token: keyboard `E`, or the PlayStation **Triangle glyph**. Both
    /// consumers (the bench / chest prompts) draw it in the icon font, so the gamepad case
    /// is the glyph itself rather than the word — see [`crate::glyph`] / [`crate::menu::PromptGlyph`].
    pub fn interact(self) -> &'static str {
        if self.pad() {
            crate::glyph::TRIANGLE
        } else {
            "E"
        }
    }
    // --- World-map hint tokens. These are drawn in the icon font (see
    // `crate::menu::PromptGlyph` / `crate::glyph`), so the gamepad side is a button glyph. ---
    /// Move the map cursor: the **D-pad** glyph, or the word `arrows`.
    pub fn map_move(self) -> &'static str {
        if self.pad() {
            crate::glyph::DPAD
        } else {
            "arrows"
        }
    }
    /// Zoom the map in one level: the **R2** trigger glyph, or `Space`.
    pub fn map_zoom_in(self) -> &'static str {
        if self.pad() {
            crate::glyph::R2
        } else {
            "Space"
        }
    }
    /// Zoom the map out one level: the **L2** trigger glyph, or `X`.
    pub fn map_zoom_out(self) -> &'static str {
        if self.pad() { crate::glyph::L2 } else { "X" }
    }
    /// Close the map / step back: the **Circle** glyph, or `Esc`.
    pub fn map_close(self) -> &'static str {
        if self.pad() {
            crate::glyph::CIRCLE
        } else {
            "Esc"
        }
    }
}

/// Gamepad buttons polled to notice controller use (for [`LastInput`]).
const PAD_BUTTONS: [GamepadButton; 12] = [
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::North,
    GamepadButton::West,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::Select,
    GamepadButton::Start,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerIntent>()
            .init_resource::<LastInput>()
            // Runs everywhere (menus included), so hints follow the device you just used.
            .add_systems(Update, track_last_input)
            .add_systems(Update, gather.in_set(GameSet::Input));
    }
}

/// Flip [`LastInput`] to whichever device produced input this frame (keyboard wins ties).
fn track_last_input(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut last: ResMut<LastInput>,
) {
    if keys.get_just_pressed().next().is_some() {
        *last = LastInput::Keyboard;
        return;
    }
    for gamepad in &gamepads {
        let stick = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0).abs() > 0.5
            || gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0).abs() > 0.5;
        if stick || PAD_BUTTONS.iter().any(|b| gamepad.just_pressed(*b)) {
            *last = LastInput::Gamepad;
            return;
        }
    }
}

/// Jump is its **own dedicated button** (no longer shared with Up): keyboard `Space`,
/// gamepad `South`. Up/Down are free for look-up / crouch (and the pogo down-slash).
const JUMP_KEYS: [KeyCode; 1] = [KeyCode::Space];

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
    let mut up = keys.any_pressed([KeyCode::ArrowUp, KeyCode::KeyW]);
    let mut down = keys.any_pressed([KeyCode::ArrowDown, KeyCode::KeyS]);
    let mut down_pressed = keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]);
    let mut jump_pressed = keys.any_just_pressed(JUMP_KEYS);
    let mut jump_held = keys.any_pressed(JUMP_KEYS);
    let mut interact = keys.just_pressed(KeyCode::KeyE);
    let mut attack = keys.just_pressed(KeyCode::KeyJ);
    let mut dash = keys.any_just_pressed([KeyCode::ShiftLeft, KeyCode::KeyL]);
    let mut dash_held = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::KeyL]);

    for gamepad in &gamepads {
        let stick = gamepad.get(GamepadAxis::LeftStickX).unwrap_or(0.0);
        if stick.abs() > 0.3 {
            move_x += stick;
        }
        let stick_y = gamepad.get(GamepadAxis::LeftStickY).unwrap_or(0.0);
        if gamepad.pressed(GamepadButton::DPadLeft) {
            move_x -= 1.0;
        }
        if gamepad.pressed(GamepadButton::DPadRight) {
            move_x += 1.0;
        }
        if gamepad.pressed(GamepadButton::DPadUp) || stick_y > 0.5 {
            up = true;
        }
        if gamepad.pressed(GamepadButton::DPadDown) || stick_y < -0.5 {
            down = true;
        }
        if gamepad.just_pressed(GamepadButton::DPadDown) {
            down_pressed = true;
        }
        if gamepad.just_pressed(GamepadButton::South) {
            jump_pressed = true;
        }
        if gamepad.pressed(GamepadButton::South) {
            jump_held = true;
        }
        if gamepad.just_pressed(GamepadButton::North) {
            interact = true;
        }
        if gamepad.just_pressed(GamepadButton::West) {
            attack = true;
        }
        if gamepad.just_pressed(GamepadButton::RightTrigger) {
            dash = true;
        }
        if gamepad.pressed(GamepadButton::RightTrigger) {
            dash_held = true;
        }
    }

    intent.move_x = move_x.clamp(-1.0, 1.0);
    intent.up = up;
    intent.down = down;
    intent.down_pressed = down_pressed;
    intent.jump_pressed = jump_pressed;
    intent.jump_held = jump_held;
    intent.interact = interact;
    intent.attack_pressed = attack;
    intent.dash_pressed = dash;
    intent.dash_held = dash_held;
}
