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
//! * **Wall slide + wall jump** (Hollow-Knight style) — touch a wall in the air to
//!   **auto-cling** and slow your fall; press **away** to let go. Jumping launches up and
//!   away from the wall (a brief control lockout keeps the launch), so you can zig-zag
//!   between facing walls.
//! * **Crouch + crouch-walk** — hold **Down** on the ground to shrink the hitbox (it shrinks
//!   from the top, feet planted) so you fit under a one-tile gap or a passing platform; add a
//!   direction to crouch-walk at [`MovementConfig::crouch_speed`]. A platform **descending onto
//!   you from overhead** also forces a duck, and only crushes ([`Hurt`]) you if it keeps coming
//!   and bites even the crouched box — riding a platform up or pressing one's side never forces
//!   a crouch.

use bevy::prelude::*;

use crate::GameSet;
use crate::audio::{PlaySfx, Sfx};
use crate::health::{Hurt, Stun};
use crate::input::PlayerIntent;
use crate::physics::{self, Platforms, Solids};
use crate::save::Save;
use crate::state::GameState;

/// Collision half-extents (the sprite is a touch larger).
pub const PLAYER_HALF: Vec2 = Vec2::new(11.0, 19.0);

/// Collision half-extents while crouching — same width, ~⅔ the height (matching the squashed
/// crouch pose), so the player fits under a one-tile gap (a low ceiling or a passing
/// platform). The box shrinks from the **top** with the feet planted (see [`movement`]).
pub const CROUCH_HALF: Vec2 = Vec2::new(11.0, 12.0);

/// Seconds between footstep sounds while running on the ground.
const FOOTSTEP_INTERVAL: f32 = 0.30;

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
    /// The side of a wall the player is currently sliding on: `-1` left, `+1` right,
    /// `0` none. Read for facing/animation.
    wall: f32,
    /// Seconds of reduced horizontal control after a wall jump, so the launch off the
    /// wall isn't instantly cancelled by holding back toward it.
    wall_lock: f32,
    /// Countdown to the next footstep sound while walking on the ground.
    step_timer: f32,
    /// Seconds left in the current dash (0 = not dashing).
    dash: f32,
    /// Seconds until another dash is allowed.
    dash_cd: f32,
    /// Travel direction of the active dash (-1 / +1).
    dash_dir: f32,
    /// An unused mid-air dash is available (refreshed on landing).
    air_dash: bool,
    /// Sprinting (dash button held while moving). Decided on the ground, then carried
    /// through jumps/falls so the run's momentum survives the air and resumes on landing.
    running: bool,
    /// Crouched this frame: Down held on the ground, or kept crouched while a low ceiling
    /// blocks standing up. Shrinks the hitbox and caps speed to a crouch-walk.
    crouching: bool,
}

impl JumpState {
    /// Whether the player is standing on the ground (read by animation).
    pub fn grounded(&self) -> bool {
        self.grounded
    }

    /// Whether the player is crouched (read by animation for the crouch / crouch-walk poses).
    pub fn crouching(&self) -> bool {
        self.crouching
    }

    /// Whether the player is sprinting (read by animation for the run cycle).
    pub fn running(&self) -> bool {
        self.running
    }

    /// Begin a **pogo** bounce (a down-slash connected): behave like a fresh jump — so
    /// variable height and the jump arc apply — and refresh the mid-air jump, so pogos can
    /// be chained and you can still double-jump out of one. Called from [`crate::combat`].
    pub fn start_pogo(&mut self) {
        self.jumping = true;
        self.air_jump = true;
    }
}

/// One acquirable traversal/combat ability. A fresh game has **none** of these (only the
/// base single jump + slash); each is granted by a boss or a chest the level designer places.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Ability {
    /// A second jump in mid-air.
    #[default]
    DoubleJump,
    /// Cling to walls and jump off them.
    WallJump,
    /// A quick horizontal burst.
    Dash,
    /// Bounce off enemies/hazards with a down-slash.
    Pogo,
}

impl Ability {
    /// All abilities, in display order.
    pub const ALL: [Ability; 4] = [
        Ability::DoubleJump,
        Ability::WallJump,
        Ability::Dash,
        Ability::Pogo,
    ];

    /// The stable key used in save files and map data.
    pub fn key(self) -> &'static str {
        match self {
            Ability::DoubleJump => "double_jump",
            Ability::WallJump => "wall_jump",
            Ability::Dash => "dash",
            Ability::Pogo => "pogo",
        }
    }

    /// A human-friendly name for menus.
    pub fn label(self) -> &'static str {
        match self {
            Ability::DoubleJump => "Double Jump",
            Ability::WallJump => "Wall Jump",
            Ability::Dash => "Dash",
            Ability::Pogo => "Pogo",
        }
    }

    /// Parse a [`key`](Ability::key).
    pub fn parse(s: &str) -> Option<Ability> {
        Ability::ALL.into_iter().find(|a| a.key() == s)
    }
}

/// Which abilities the player has unlocked (persisted in the [`Save`]). All false on a new
/// game; granted by bosses / chests (see [`crate::boss`], [`crate::world`]).
#[derive(Resource, Default, Clone, Copy)]
pub struct Abilities {
    pub double_jump: bool,
    pub wall_jump: bool,
    pub dash: bool,
    pub pogo: bool,
}

impl Abilities {
    pub fn has(&self, a: Ability) -> bool {
        match a {
            Ability::DoubleJump => self.double_jump,
            Ability::WallJump => self.wall_jump,
            Ability::Dash => self.dash,
            Ability::Pogo => self.pogo,
        }
    }

    pub fn grant(&mut self, a: Ability) {
        self.set(a, true);
    }

    pub fn set(&mut self, a: Ability, on: bool) {
        match a {
            Ability::DoubleJump => self.double_jump = on,
            Ability::WallJump => self.wall_jump = on,
            Ability::Dash => self.dash = on,
            Ability::Pogo => self.pogo = on,
        }
    }

    /// Build from a comma-separated list of [`Ability::key`]s (a save field).
    pub fn from_csv(s: &str) -> Abilities {
        let mut out = Abilities::default();
        for token in s.split(',') {
            if let Some(a) = Ability::parse(token.trim()) {
                out.grant(a);
            }
        }
        out
    }

    /// Serialise to the comma-separated save form.
    pub fn to_csv(self) -> String {
        Ability::ALL
            .iter()
            .filter(|a| self.has(**a))
            .map(|a| a.key())
            .collect::<Vec<_>>()
            .join(",")
    }
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
    /// Max fall speed while sliding down a wall (slower than a free fall).
    pub wall_slide_speed: f32,
    /// Horizontal launch speed away from the wall on a wall jump.
    pub wall_jump_x: f32,
    /// Seconds of reduced horizontal control right after a wall jump.
    pub wall_jump_lock: f32,
    /// Dash burst speed, duration, and cooldown.
    pub dash_speed: f32,
    pub dash_time: f32,
    pub dash_cd: f32,
    /// Sustained run speed when the dash button stays held after a dash (> `run_speed`).
    pub sprint_speed: f32,
    /// Crouch-walk speed (Down held while moving) — slower than `run_speed`, reduced hitbox.
    pub crouch_speed: f32,
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
            wall_slide_speed: 120.0,
            wall_jump_x: 250.0,
            wall_jump_lock: 0.16,
            dash_speed: 560.0,
            dash_time: 0.16,
            dash_cd: 0.45,
            sprint_speed: 420.0,
            crouch_speed: 130.0,
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
    *abilities = Abilities::from_csv(&save.abilities);
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if current < target {
        (current + max_delta).min(target)
    } else {
        (current - max_delta).max(target)
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct resource/query
pub(crate) fn movement(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    cfg: Res<MovementConfig>,
    solids: Res<Solids>,
    platforms: Res<Platforms>,
    stun: Res<Stun>,
    abilities: Res<Abilities>,
    mut hurt: MessageWriter<Hurt>,
    mut sfx: MessageWriter<PlaySfx>,
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
        // Brief reduced-control window after a wall jump (so the launch isn't cancelled
        // by immediately holding back toward the wall).
        let locked = jump.wall_lock > 0.0;
        jump.wall_lock = (jump.wall_lock - dt).max(0.0);

        // --- dash: a quick horizontal burst once unlocked; air-dash refreshes on landing ---
        jump.dash = (jump.dash - dt).max(0.0);
        jump.dash_cd = (jump.dash_cd - dt).max(0.0);
        if jump.grounded {
            jump.air_dash = true;
        }
        if intent.dash_pressed
            && abilities.dash
            && !stunned
            && jump.dash <= 0.0
            && jump.dash_cd <= 0.0
            && (jump.grounded || jump.air_dash)
        {
            jump.dash = cfg.dash_time;
            jump.dash_cd = cfg.dash_cd;
            jump.dash_dir = if intent.move_x.abs() > 0.1 {
                intent.move_x.signum()
            } else if sprite.flip_x {
                -1.0
            } else {
                1.0
            };
            if !jump.grounded {
                jump.air_dash = false;
            }
            sfx.write(PlaySfx(Sfx::Dash));
        }
        let dashing = jump.dash > 0.0;

        // --- crouch: hold Down on the ground to shrink the hitbox (fit under a one-tile gap or
        // a passing platform) and slow to a crouch-walk. A platform **descending onto you from
        // overhead** also forces a duck (then a crush — see the squish below — if even the
        // crouched box won't fit). Riding a platform up, or pressing against one's side, must
        // *not* force a crouch, so the force is gated on `ducking_under`, not "anything
        // overlapping the standing box". Once crouched, you stay down while a ceiling blocks
        // standing up. The box shrinks from the top, feet planted. ---
        let body = transform.translation.truncate();
        let can_stand = !physics::blocked(&solids, &platforms, body, PLAYER_HALF);
        let force_duck = physics::ducking_under(&platforms, body, PLAYER_HALF);
        // Crouch on the ground (Down / forced duck); stay crouched — **even in the air** — while
        // a low ceiling or platform blocks standing, so jumping up under one keeps the small box
        // (you bonk it) instead of un-crouching into it and registering a false crush.
        let crouching = !dashing
            && !stunned
            && ((jump.grounded && (intent.down || force_duck)) || (jump.crouching && !can_stand));
        jump.crouching = crouching;
        let half = if crouching { CROUCH_HALF } else { PLAYER_HALF };
        let crouch_drop = PLAYER_HALF.y - half.y;

        // Sprint state: decided while grounded (dash button held + moving), then left
        // untouched in the air so a jump/fall carries the run's momentum and resumes it on
        // landing. Only re-evaluated once back on the ground. Crouching suppresses the sprint.
        if jump.grounded {
            jump.running = abilities.dash
                && intent.dash_held
                && !stunned
                && !crouching
                && intent.move_x.abs() > 0.1;
        }

        if !stunned && !dashing {
            // --- wall cling (Hollow-Knight style): grab automatically on contact while
            // airborne; let go by pressing *away* from the wall (or off the ground). ---
            let here = transform.translation.truncate();
            let wall_dir = if jump.grounded || !abilities.wall_jump {
                0.0 // wall slide/jump must be unlocked first
            } else if intent.move_x > -0.1
                && physics::wall_at(&solids, &platforms, here, PLAYER_HALF, 1.0)
            {
                1.0 // right wall — clings unless you hold left (away)
            } else if intent.move_x < 0.1
                && physics::wall_at(&solids, &platforms, here, PLAYER_HALF, -1.0)
            {
                -1.0 // left wall — clings unless you hold right (away)
            } else {
                0.0
            };
            jump.wall = wall_dir;

            // --- horizontal (coasts during the wall-jump lockout) ---
            if !locked {
                // Running (see above) sustains a faster sprint speed; the run state carries
                // through the air, so a jump keeps its momentum and a landing keeps running.
                let speed = if crouching {
                    cfg.crouch_speed
                } else if jump.running {
                    cfg.sprint_speed
                } else {
                    cfg.run_speed
                };
                let target = intent.move_x * speed;
                let accel = if jump.grounded {
                    cfg.accel_ground
                } else {
                    cfg.accel_air
                };
                velocity.0.x = approach(velocity.0.x, target, accel * dt);
            }

            // --- facing: toward the wall while clinging, else launch / input ---
            if wall_dir != 0.0 {
                sprite.flip_x = wall_dir < 0.0;
            } else if locked {
                sprite.flip_x = velocity.0.x < 0.0;
            } else if intent.move_x > 0.05 {
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

            // --- start a jump: ground (coyote) → wall → double ---
            if jump.buffer > 0.0 && jump.coyote > 0.0 {
                velocity.0.y = cfg.jump_speed;
                jump.jumping = true;
                jump.buffer = 0.0;
                jump.coyote = 0.0;
                sfx.write(PlaySfx(Sfx::Jump));
            } else if jump.buffer > 0.0 && wall_dir != 0.0 {
                // wall jump: up and away from the wall, with a short control lockout
                velocity.0.y = cfg.jump_speed;
                velocity.0.x = -wall_dir * cfg.wall_jump_x;
                jump.jumping = true;
                jump.buffer = 0.0;
                jump.wall = 0.0;
                jump.wall_lock = cfg.wall_jump_lock;
                sfx.write(PlaySfx(Sfx::WallJump));
            } else if intent.jump_pressed
                && !jump.grounded
                && abilities.double_jump
                && jump.air_jump
            {
                // --- double jump: a fresh press in the air, once per airtime ---
                velocity.0.y = cfg.jump_speed;
                jump.jumping = true;
                jump.air_jump = false;
                sfx.write(PlaySfx(Sfx::DoubleJump));
            }

            // --- variable height: releasing early cuts the rise ---
            if !intent.jump_held && jump.jumping && velocity.0.y > 0.0 {
                velocity.0.y *= cfg.jump_cut;
                jump.jumping = false;
            }
            if velocity.0.y <= 0.0 {
                jump.jumping = false;
            }
        } else {
            jump.wall = 0.0;
        }

        if dashing {
            // A flat horizontal zip: fixed speed, no gravity, no steering.
            velocity.0.x = jump.dash_dir * cfg.dash_speed;
            velocity.0.y = 0.0;
            sprite.flip_x = jump.dash_dir < 0.0;
        } else {
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

            // --- wall slide: cap the fall speed while clinging ---
            if jump.wall != 0.0 && velocity.0.y < -cfg.wall_slide_speed {
                velocity.0.y = -cfg.wall_slide_speed;
            }
        }

        // --- integrate + collide (X then Y), against the static grid then any moving
        // platforms. A platform the player is standing on first carries them along. ---
        let was_grounded = jump.grounded;
        // Collide a feet-anchored box: drop the centre by `crouch_drop` so the shrunk crouch
        // box keeps its feet on the ground (it shrinks from the top); the offset is restored
        // on write-back, so the transform — and thus the sprite — never moves when crouching.
        let mut center = transform.translation.truncate() - Vec2::new(0.0, crouch_drop);
        // A platform the player rides carries them along. Fold its **horizontal** carry into
        // the X move so it's collision-checked too — otherwise the rider is teleported sideways
        // and can be slid through a wall / low ceiling the platform passes under (e.g. squeezing
        // a standing player through a one-tile gap that should require a crouch). The vertical
        // carry stays a direct shift, so gravity still re-seats the feet for grounding.
        let carry = physics::carry_delta(&platforms, center, half).unwrap_or(Vec2::ZERO);
        center.y += carry.y;

        let dx = carry.x + velocity.0.x * dt;
        // Platforms on X: push side hits out; a moving platform driving you into a wall (with no
        // room to duck) is a **horizontal crush** — hurt, not clipped into the wall.
        let mut blocked_x = physics::collide_x(&solids, &mut center, half, dx);
        let (px_blocked, x_crush) =
            physics::resolve_platforms_x(&solids, &platforms, &mut center, half);
        blocked_x |= px_blocked;
        if blocked_x {
            velocity.0.x = 0.0;
        }

        let dy = velocity.0.y * dt;
        let (sb, sl) = physics::collide_y(&solids, &mut center, half, dy);

        // Platforms on Y: land on tops, bonk heads, and report a **vertical crush** (a platform
        // pressing you down with no room below) as a squish — hurt, not teleported. Crouching
        // shrinks `half`, so a platform that clears the lowered head doesn't register at all.
        let (pb, pl, y_crush) =
            physics::resolve_platforms_y(&solids, &platforms, &mut center, half);
        if sb || pb {
            velocity.0.y = 0.0;
        }
        if let Some(src) = x_crush.or(y_crush) {
            hurt.write(Hurt::From(src));
        }
        jump.grounded = sl || pl;

        // --- footsteps + landing thump ---
        if !was_grounded && jump.grounded && dy < 0.0 {
            sfx.write(PlaySfx(Sfx::Land));
        }
        if jump.grounded && velocity.0.x.abs() > 20.0 {
            jump.step_timer -= dt;
            if jump.step_timer <= 0.0 {
                sfx.write(PlaySfx(Sfx::Footstep));
                jump.step_timer = FOOTSTEP_INTERVAL;
            }
        } else {
            jump.step_timer = 0.0; // so the first step on moving off lands promptly
        }

        transform.translation.x = center.x;
        transform.translation.y = center.y + crouch_drop;
    }
}
