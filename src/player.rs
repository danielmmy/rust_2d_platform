//! The player: feel-tuned movement and jumping.
//!
//! The "nice jump" comes from a few classic platformer tricks, all tunable via
//! [`MovementConfig`]:
//!
//! * **Coyote time** — you can still jump for a moment after walking off a ledge.
//! * **Jump buffering** — a jump pressed just before landing still fires.
//! * **Variable height** — releasing jump early cuts the rise short.
//! * **Asymmetric gravity** — you fall faster than you rise (snappy, not floaty).
//! * **Apex control** — slightly reduced gravity near the peak for air control.

use bevy::prelude::*;

use crate::GameSet;
use crate::health::Stun;
use crate::input::PlayerIntent;
use crate::physics::{self, Solids};
use crate::save::Save;
use crate::state::GameState;

/// Collision half-extents (the sprite is a touch larger).
pub const PLAYER_HALF: Vec2 = Vec2::new(11.0, 19.0);

#[derive(Component)]
pub struct Player;

#[derive(Component, Default)]
pub struct Velocity(pub Vec2);

#[derive(Component, Default)]
pub struct JumpState {
    coyote: f32,
    buffer: f32,
    jumping: bool,
    grounded: bool,
    /// An unused mid-air jump is available (refreshed on landing; spent by the
    /// double jump). Only usable once the ability is unlocked — see [`Abilities`].
    air_jump: bool,
}

impl JumpState {
    /// Whether the player is standing on the ground (read by animation).
    pub fn grounded(&self) -> bool {
        self.grounded
    }
}

/// Unlockable player abilities (persisted in the [`Save`]).
#[derive(Resource, Default)]
pub struct Abilities {
    /// A second jump in mid-air — the reward for beating the boss.
    pub double_jump: bool,
}

/// Every knob that shapes how movement feels. Tweak and re-run to taste.
#[derive(Resource)]
pub struct MovementConfig {
    pub run_speed: f32,
    pub accel_ground: f32,
    pub accel_air: f32,
    pub jump_speed: f32,
    pub gravity_up: f32,
    pub gravity_down: f32,
    pub max_fall: f32,
    pub coyote_time: f32,
    pub jump_buffer: f32,
    pub jump_cut: f32,
    pub apex_speed: f32,
    pub apex_gravity_mult: f32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            run_speed: 270.0,
            accel_ground: 2600.0,
            accel_air: 1500.0,
            jump_speed: 560.0,
            gravity_up: 1500.0,
            gravity_down: 2500.0,
            max_fall: 900.0,
            coyote_time: 0.10,
            jump_buffer: 0.12,
            jump_cut: 0.45,
            apex_speed: 70.0,
            apex_gravity_mult: 0.55,
        }
    }
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementConfig>()
            .init_resource::<Abilities>()
            .add_systems(OnEnter(GameState::Playing), apply_abilities)
            .add_systems(Update, movement.in_set(GameSet::Movement));
    }
}

/// Push the save's unlocked abilities into the live resource on entering play.
fn apply_abilities(save: Res<Save>, mut abilities: ResMut<Abilities>) {
    abilities.double_jump = save.double_jump;
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if current < target {
        (current + max_delta).min(target)
    } else {
        (current - max_delta).max(target)
    }
}

fn movement(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    cfg: Res<MovementConfig>,
    solids: Res<Solids>,
    stun: Res<Stun>,
    abilities: Res<Abilities>,
    mut query: Query<(&mut Transform, &mut Velocity, &mut JumpState, &mut Sprite), With<Player>>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    // While stunned (knocked back from a hit) the player has no steering — they just
    // coast under the knockback velocity + gravity.
    let stunned = stun.0 > 0.0;

    for (mut transform, mut velocity, mut jump, mut sprite) in &mut query {
        if !stunned {
            // --- horizontal ---
            let target = intent.move_x * cfg.run_speed;
            let accel = if jump.grounded {
                cfg.accel_ground
            } else {
                cfg.accel_air
            };
            velocity.0.x = approach(velocity.0.x, target, accel * dt);
            if intent.move_x > 0.05 {
                sprite.flip_x = false;
            } else if intent.move_x < -0.05 {
                sprite.flip_x = true;
            }

            // --- coyote time + jump buffer ---
            if jump.grounded {
                jump.coyote = cfg.coyote_time;
                jump.air_jump = true; // a fresh mid-air jump on every landing
            } else {
                jump.coyote = (jump.coyote - dt).max(0.0);
            }
            if intent.jump_pressed {
                jump.buffer = cfg.jump_buffer;
            } else {
                jump.buffer = (jump.buffer - dt).max(0.0);
            }

            // --- start a jump ---
            if jump.buffer > 0.0 && jump.coyote > 0.0 {
                velocity.0.y = cfg.jump_speed;
                jump.jumping = true;
                jump.buffer = 0.0;
                jump.coyote = 0.0;
            } else if intent.jump_pressed
                && !jump.grounded
                && abilities.double_jump
                && jump.air_jump
            {
                // --- double jump: a fresh press in the air, once per airtime ---
                velocity.0.y = cfg.jump_speed;
                jump.jumping = true;
                jump.air_jump = false;
            }

            // --- variable height: releasing early cuts the rise ---
            if !intent.jump_held && jump.jumping && velocity.0.y > 0.0 {
                velocity.0.y *= cfg.jump_cut;
                jump.jumping = false;
            }
            if velocity.0.y <= 0.0 {
                jump.jumping = false;
            }
        }

        // --- gravity (asymmetric + apex control) ---
        let gravity = if velocity.0.y > 0.0 {
            if velocity.0.y < cfg.apex_speed {
                cfg.gravity_up * cfg.apex_gravity_mult
            } else {
                cfg.gravity_up
            }
        } else {
            cfg.gravity_down
        };
        velocity.0.y = (velocity.0.y - gravity * dt).max(-cfg.max_fall);

        // --- integrate + collide (X then Y) ---
        let mut center = transform.translation.truncate();
        if physics::collide_x(&solids, &mut center, PLAYER_HALF, velocity.0.x * dt) {
            velocity.0.x = 0.0;
        }
        let (blocked, landed) =
            physics::collide_y(&solids, &mut center, PLAYER_HALF, velocity.0.y * dt);
        if blocked {
            velocity.0.y = 0.0;
        }
        jump.grounded = landed;

        transform.translation.x = center.x;
        transform.translation.y = center.y;
    }
}
