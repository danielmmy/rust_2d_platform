//! Enemies, energy drops, and the player's sword combo.
//!
//! **Enemies** ([`Enemy`]) are spawned from the map's `E` glyph (by
//! [`crate::world`]); they patrol on the ground (turning at walls and ledges) and
//! carry a [`Hazard`](crate::hazards) so they hurt the player on contact. They
//! respawn whenever the room reloads — including when you rest at a bench.
//!
//! The player swings with **`J`** (gamepad `X`): a hitbox in front of the facing
//! direction. Presses chain a **3-hit combo** while the window is open. A killed
//! enemy drops an **energy orb**; walking over it adds to [`Energy`] (shown on the
//! HUD).

use std::collections::HashSet;

use bevy::prelude::*;

use crate::GameSet;
use crate::anim::{Clip, Playback};
use crate::audio::{PlaySfx, Sfx};
use crate::boss::{BOSS_HALF, Boss, BossFight};
use crate::hazards::Hazard;
use crate::input::PlayerIntent;
use crate::physics::{self, Solids};
use crate::player::{JumpState, PLAYER_HALF, Player, Velocity};
use crate::save::Save;
use crate::state::GameState;
use crate::stats::{self, Stats};
use crate::world::{GameAssets, MapEntity};

// --- enemies -------------------------------------------------------------

/// Half-extents of an enemy (collision + contact).
pub(crate) const ENEMY_HALF: Vec2 = Vec2::new(10.0, 13.0);
const ENEMY_GRAVITY: f32 = 1400.0;
const ENEMY_MAX_FALL: f32 = 800.0;
/// A drifting flyer's vertical bob speed, as a fraction of its horizontal speed.
const FLY_BOB_RATIO: f32 = 0.6;

/// How an enemy decides where to move.
#[derive(Clone, Copy)]
pub(crate) enum EnemyAi {
    /// Walk back and forth, turning at walls and ledge edges.
    Patrol,
    /// Chase the player while within `aggro` pixels; patrol otherwise.
    Chase { aggro: f32 },
    /// Amble until the player is within `aggro`, then leap toward them (a hop
    /// onto their head) with vertical speed `jump`. After a leap it waits
    /// `cooldown` seconds before it can jump again, so it can't spam pounces.
    Pounce {
        aggro: f32,
        jump: f32,
        cooldown: f32,
    },
    /// Flies (no gravity): cruises horizontally and bobs vertically, bouncing
    /// off any solid it meets.
    Drift,
    /// Flies (no gravity): homes straight in on the player while within `aggro`;
    /// otherwise drifts like [`Drift`](EnemyAi::Drift).
    Hunt { aggro: f32 },
}

impl EnemyAi {
    /// Whether this behaviour moves freely through the air (ignoring gravity and
    /// ground/ledge handling).
    fn flies(self) -> bool {
        matches!(self, EnemyAi::Drift | EnemyAi::Hunt { .. })
    }
}

/// A fully data-driven enemy type: stats, behaviour ([`ai`](EnemyKind::ai)), and
/// look — the `sheet` (an `assets/sprites/` file) gridded `cols`×`rows` and played
/// with `clip`, tinted to `color`. Add a kind by appending one entry; it needs no
/// extra glyph (the map's `enemies` array references kinds by index) and no code.
pub(crate) struct EnemyKind {
    pub(crate) health: i32,
    pub(crate) color: Color,
    pub(crate) speed: f32,
    pub(crate) ai: EnemyAi,
    pub(crate) sheet: &'static str,
    pub(crate) cols: u32,
    pub(crate) rows: u32,
    pub(crate) clip: Clip,
}

/// The enemy types, indexed by the `kind` in a map's `enemies` array. Index 0 is
/// the default when an `E` glyph has no matching entry.
pub(crate) const ENEMY_KINDS: [EnemyKind; 6] = [
    // Basic: ambling purple patroller.
    EnemyKind {
        health: 3,
        color: Color::srgb(0.70, 0.30, 0.64),
        speed: 56.0,
        ai: EnemyAi::Patrol,
        sheet: "enemy.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 6.0,
            playback: Playback::Loop,
        },
    },
    // Red: faster, and chases the player when close.
    EnemyKind {
        health: 2,
        color: Color::srgb(0.86, 0.27, 0.27),
        speed: 84.0,
        ai: EnemyAi::Chase { aggro: 150.0 },
        sheet: "enemy.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 10.0,
            playback: Playback::Loop,
        },
    },
    // Green leaper: hops toward the player, aiming for their head.
    EnemyKind {
        health: 2,
        color: Color::srgb(0.45, 0.78, 0.42),
        speed: 92.0,
        ai: EnemyAi::Pounce {
            aggro: 170.0,
            jump: 380.0,
            cooldown: 1.4,
        },
        sheet: "jumper.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 5.0,
            playback: Playback::Loop,
        },
    },
    // Amber leaper: a beefier, harder-hitting jumper — more health, a stronger,
    // farther-reaching pounce, but a slightly longer recovery between leaps.
    EnemyKind {
        health: 4,
        color: Color::srgb(0.93, 0.62, 0.24),
        speed: 104.0,
        ai: EnemyAi::Pounce {
            aggro: 210.0,
            jump: 430.0,
            cooldown: 1.7,
        },
        sheet: "jumper.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 6.0,
            playback: Playback::Loop,
        },
    },
    // Cyan flutterer: drifts around the room, bouncing off walls and floors.
    EnemyKind {
        health: 2,
        color: Color::srgb(0.40, 0.80, 0.86),
        speed: 70.0,
        ai: EnemyAi::Drift,
        sheet: "flyer.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 9.0,
            playback: Playback::Loop,
        },
    },
    // Magenta stalker: flies straight at the player once they're in range.
    EnemyKind {
        health: 2,
        color: Color::srgb(0.88, 0.36, 0.74),
        speed: 96.0,
        ai: EnemyAi::Hunt { aggro: 220.0 },
        sheet: "flyer.png",
        cols: 2,
        rows: 1,
        clip: Clip {
            first: 0,
            count: 2,
            fps: 12.0,
            playback: Playback::Loop,
        },
    },
];

/// A spawned enemy: its [`kind`](Enemy::kind) (indexing [`ENEMY_KINDS`]), remaining
/// health, facing/move direction, and vertical velocity.
#[derive(Component)]
pub(crate) struct Enemy {
    pub(crate) kind: usize,
    health: i32,
    dir: f32,
    vy: f32,
    /// Seconds remaining before a [`Pounce`](EnemyAi::Pounce) enemy may leap again.
    jump_cd: f32,
}

impl Enemy {
    /// A fresh enemy of `kind`, walking right. Flyers start with an upward drift
    /// velocity so they bob immediately; grounded kinds begin at rest.
    pub(crate) fn new(kind: usize) -> Self {
        let kind = kind.min(ENEMY_KINDS.len() - 1);
        let spec = &ENEMY_KINDS[kind];
        Self {
            kind,
            health: spec.health,
            dir: 1.0,
            vy: if spec.ai.flies() {
                spec.speed * FLY_BOB_RATIO
            } else {
                0.0
            },
            jump_cd: 0.0,
        }
    }
}

/// Move each enemy per its kind's AI (patrol or chase), with gravity and
/// wall/ledge handling.
fn enemy_ai(
    time: Res<Time>,
    solids: Res<Solids>,
    players: Query<&Transform, (With<Player>, Without<Enemy>)>,
    mut enemies: Query<(&mut Transform, &mut Enemy, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let player_pos = players.single().ok().map(|t| t.translation.truncate());

    for (mut transform, mut enemy, mut sprite) in &mut enemies {
        let kind = &ENEMY_KINDS[enemy.kind];
        let mut center = transform.translation.truncate();

        // --- Flying enemies: free aerial movement, no gravity. ---
        if kind.ai.flies() {
            // Aerial chasers home straight in on the player while in range.
            let hunting = match kind.ai {
                EnemyAi::Hunt { aggro } => player_pos.is_some_and(|pp| center.distance(pp) < aggro),
                _ => false,
            };
            if hunting {
                let to = (player_pos.unwrap() - center).normalize_or_zero();
                physics::collide_x(&solids, &mut center, ENEMY_HALF, to.x * kind.speed * dt);
                physics::collide_y(&solids, &mut center, ENEMY_HALF, to.y * kind.speed * dt);
                if to.x != 0.0 {
                    enemy.dir = to.x.signum();
                }
            } else {
                // Drift: cruise horizontally and bob vertically, reversing on contact.
                if physics::collide_x(
                    &solids,
                    &mut center,
                    ENEMY_HALF,
                    enemy.dir * kind.speed * dt,
                ) {
                    enemy.dir = -enemy.dir;
                }
                let (vblocked, _) =
                    physics::collide_y(&solids, &mut center, ENEMY_HALF, enemy.vy * dt);
                if vblocked {
                    enemy.vy = -enemy.vy;
                }
            }
            transform.translation.x = center.x;
            transform.translation.y = center.y;
            sprite.flip_x = enemy.dir < 0.0;
            continue;
        }

        let grounded = solids.solid_at(center.x, center.y - ENEMY_HALF.y - 2.0);
        enemy.jump_cd = (enemy.jump_cd - dt).max(0.0);

        // Is the player within this kind's aggro range? If so, face them.
        let engaged = |enemy: &mut Enemy| -> bool {
            let aggro = match kind.ai {
                EnemyAi::Chase { aggro } | EnemyAi::Pounce { aggro, .. } => aggro,
                // Flying kinds are handled above and never reach this closure.
                EnemyAi::Patrol | EnemyAi::Drift | EnemyAi::Hunt { .. } => return false,
            };
            match player_pos {
                Some(pp) if center.distance(pp) < aggro => {
                    enemy.dir = if pp.x >= center.x { 1.0 } else { -1.0 };
                    true
                }
                _ => false,
            }
        };
        let engaged = engaged(&mut enemy);

        // A pouncer that's engaged, grounded and off cooldown leaps at the player.
        if let EnemyAi::Pounce { jump, cooldown, .. } = kind.ai
            && engaged
            && grounded
            && enemy.jump_cd <= 0.0
        {
            enemy.vy = jump;
            enemy.jump_cd = cooldown;
        }

        // A wall or ledge directly ahead in the current facing.
        let ahead = center.x + enemy.dir * (ENEMY_HALF.x + 2.0);
        let wall = solids.solid_at(ahead, center.y);
        let ledge = grounded && !solids.solid_at(ahead, center.y - ENEMY_HALF.y - 2.0);

        let mut step = enemy.dir * kind.speed * dt;
        match kind.ai {
            // Patrollers (and idle chasers) reverse at walls and ledges.
            EnemyAi::Patrol => {
                if wall || ledge {
                    enemy.dir = -enemy.dir;
                    step = enemy.dir * kind.speed * dt;
                }
            }
            EnemyAi::Chase { .. } => {
                if engaged {
                    if wall || ledge {
                        step = 0.0; // wait at the edge rather than turning or falling
                    }
                } else if wall || ledge {
                    enemy.dir = -enemy.dir;
                    step = enemy.dir * kind.speed * dt;
                }
            }
            // A pouncer only travels horizontally while airborne (its committed
            // leap); on the ground it stays put, telegraphing the next hop.
            EnemyAi::Pounce { .. } => {
                if grounded || wall {
                    step = 0.0;
                }
            }
            // Flying kinds are handled by the early `continue` above.
            EnemyAi::Drift | EnemyAi::Hunt { .. } => {}
        }

        physics::collide_x(&solids, &mut center, ENEMY_HALF, step);
        enemy.vy = (enemy.vy - ENEMY_GRAVITY * dt).max(-ENEMY_MAX_FALL);
        let (vblocked, _) = physics::collide_y(&solids, &mut center, ENEMY_HALF, enemy.vy * dt);
        if vblocked {
            enemy.vy = 0.0;
        }

        transform.translation.x = center.x;
        transform.translation.y = center.y;
        sprite.flip_x = enemy.dir < 0.0;
    }
}

/// A dead enemy (health spent) is despawned and leaves an [`EnergyOrb`].
fn enemy_death(
    mut commands: Commands,
    assets: Res<GameAssets>,
    enemies: Query<(Entity, &Transform, &Enemy)>,
) {
    for (entity, transform, enemy) in &enemies {
        if enemy.health <= 0 {
            commands.entity(entity).despawn();
            commands.spawn((
                MapEntity,
                EnergyOrb,
                Sprite {
                    image: assets.orb.clone(),
                    custom_size: Some(Vec2::splat(14.0)),
                    ..default()
                },
                Transform::from_xyz(transform.translation.x, transform.translation.y, 3.0),
            ));
        }
    }
}

// --- energy --------------------------------------------------------------

const ORB_HALF: Vec2 = Vec2::new(7.0, 7.0);
const ENERGY_PER_ORB: u32 = 1;
/// Pickup radius (half-extents) of a dropped bloodstain.
const BLOODSTAIN_HALF: Vec2 = Vec2::new(11.0, 11.0);

/// An energy pickup dropped by a dead enemy.
#[derive(Component)]
struct EnergyOrb;

/// How much energy the player has gathered (the upgrade currency). Banked into the
/// [`Save`](crate::save::Save) at rest/upgrade points; **all of it is dropped as a
/// [`Bloodstain`] on death** (see [`crate::stats`]).
#[derive(Resource, Default)]
pub struct Energy(pub u32);

/// Energy dropped where the player last died, recoverable by returning to it before
/// dying again. `amount == 0` (and an empty `room`) means none is pending.
#[derive(Resource, Default)]
pub(crate) struct LostEnergy {
    pub(crate) amount: u32,
    pub(crate) room: String,
    pub(crate) pos: Vec2,
}

/// The visible marker for [`LostEnergy`]; touching it refunds the energy. Tagged
/// [`MapEntity`] so it's re-spawned by the loader each time its room is entered.
#[derive(Component)]
pub(crate) struct Bloodstain;

fn collect_energy(
    mut commands: Commands,
    mut energy: ResMut<Energy>,
    mut sfx: MessageWriter<PlaySfx>,
    player: Query<&Transform, With<Player>>,
    orbs: Query<(Entity, &Transform), With<EnergyOrb>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    for (entity, transform) in &orbs {
        let delta = (transform.translation.truncate() - player_pos).abs();
        if delta.x < ORB_HALF.x + PLAYER_HALF.x && delta.y < ORB_HALF.y + PLAYER_HALF.y {
            energy.0 += ENERGY_PER_ORB;
            commands.entity(entity).despawn();
            sfx.write(PlaySfx(Sfx::Pickup));
        }
    }
}

/// Walking over your dropped bloodstain refunds the lost energy and clears it
/// (persisted so a reload can't reclaim it twice).
fn recover_lost_energy(
    mut commands: Commands,
    mut energy: ResMut<Energy>,
    mut lost: ResMut<LostEnergy>,
    stats: Res<Stats>,
    mut save: ResMut<Save>,
    player: Query<&Transform, With<Player>>,
    stains: Query<(Entity, &Transform), With<Bloodstain>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    for (entity, transform) in &stains {
        let delta = (transform.translation.truncate() - player_pos).abs();
        if delta.x < BLOODSTAIN_HALF.x + PLAYER_HALF.x
            && delta.y < BLOODSTAIN_HALF.y + PLAYER_HALF.y
        {
            energy.0 += lost.amount;
            lost.amount = 0;
            lost.room.clear();
            commands.entity(entity).despawn();
            stats::write_progress(&mut save, &energy, &stats, &lost);
        }
    }
}

// --- sword combo ---------------------------------------------------------

/// Seconds between swings (you can't attack again until it elapses).
const SWING_COOLDOWN: f32 = 0.26;
/// Extra time after the cooldown during which a press continues the combo.
const COMBO_GRACE: f32 = 0.40;
/// How long the hitbox stays live after a swing — checked every frame (each enemy
/// hit at most once per swing), so the strike connects even if the enemy is moving.
const SWING_ACTIVE: f32 = 0.12;
/// How far in front of the player the hitbox centre sits, and its half-extents.
/// Deliberately generous (a wide arc, à la Silksong's nail) so swings feel forgiving.
const HIT_REACH: f32 = 24.0;
const HIT_HALF: Vec2 = Vec2::new(28.0, 26.0);
/// How far **below** the player a down-slash hitbox sits (the pogo strike).
const HIT_DOWN_REACH: f32 = 26.0;
/// Upward bounce speed when a down-slash connects (Hollow-Knight pogo).
const POGO_SPEED: f32 = 360.0;
/// How long a slash sprite lingers.
const SLASH_TIME: f32 = 0.12;

/// The player's sword state: combo step (0 = idle, 1..=3), its timers, and — while a
/// swing is live — its direction, whether it's a downward (pogo) strike, whether that
/// strike has already bounced, and which enemies it has already hit.
#[derive(Resource, Default)]
struct Sword {
    step: u8,
    cooldown: f32,
    combo_window: f32,
    active: f32,
    dir: f32,
    /// This swing points straight down (a mid-air pogo strike).
    down: bool,
    /// A down-swing has already bounced the player (so it pogos once per swing).
    pogoed: bool,
    hit: HashSet<Entity>,
}

/// A short-lived slash effect sprite.
#[derive(Component)]
struct Slash(f32);

#[allow(clippy::too_many_arguments, clippy::type_complexity)] // a Bevy system; distinct params
fn player_attack(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    assets: Res<GameAssets>,
    stats: Res<Stats>,
    fight: Res<BossFight>,
    mut sword: ResMut<Sword>,
    mut commands: Commands,
    mut sfx: MessageWriter<PlaySfx>,
    mut player: Query<(&Transform, &Sprite, &mut Velocity, &mut JumpState), With<Player>>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
    mut bosses: Query<(Entity, &Transform, &mut Boss), Without<Player>>,
    hazards: Query<(&Transform, &Hazard), (Without<Player>, Without<Enemy>, Without<Boss>)>,
) {
    let dt = time.delta_secs();
    sword.cooldown = (sword.cooldown - dt).max(0.0);
    sword.combo_window = (sword.combo_window - dt).max(0.0);
    sword.active = (sword.active - dt).max(0.0);
    if sword.combo_window <= 0.0 {
        sword.step = 0; // window lapsed → combo resets
    }

    let Ok((player_tf, sprite, mut velocity, mut jump)) = player.single_mut() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Start a swing on press (when off cooldown): advance the combo, re-arm timers,
    // open the hitbox window, lock its direction, and flash a slash. Holding Down while
    // airborne aims the swing **straight down** — a pogo strike.
    if intent.attack_pressed && sword.cooldown <= 0.0 {
        sword.step = if sword.combo_window > 0.0 && sword.step < 3 {
            sword.step + 1
        } else {
            1
        };
        sword.cooldown = SWING_COOLDOWN;
        sword.combo_window = SWING_COOLDOWN + COMBO_GRACE;
        sword.active = SWING_ACTIVE;
        sword.dir = if sprite.flip_x { -1.0 } else { 1.0 };
        sword.down = intent.down && !jump.grounded();
        sword.pogoed = false;
        sword.hit.clear();
        // A heftier sound on the third hit (the combo finisher).
        sfx.write(PlaySfx(if sword.step >= 3 {
            Sfx::SlashHeavy
        } else {
            Sfx::Slash
        }));

        // Slash visual — roughly matches the hitbox; bigger and gold on the finisher.
        let (size, color) = match sword.step {
            3 => (Vec2::new(58.0, 58.0), Color::srgb(1.0, 0.85, 0.4)),
            2 => (Vec2::new(52.0, 52.0), Color::srgb(0.85, 0.95, 1.0)),
            _ => (Vec2::new(46.0, 48.0), Color::srgb(0.6, 0.9, 1.0)),
        };
        let (offset, rotation, flip) = if sword.down {
            // Point the slash downward, under the player's feet.
            (
                Vec2::new(0.0, -HIT_DOWN_REACH),
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                false,
            )
        } else {
            (
                Vec2::new(sword.dir * HIT_REACH, 0.0),
                Quat::IDENTITY,
                sword.dir < 0.0,
            )
        };
        commands.spawn((
            MapEntity,
            Slash(SLASH_TIME),
            Sprite {
                image: assets.slash.clone(),
                custom_size: Some(size),
                color,
                flip_x: flip,
                ..default()
            },
            Transform {
                translation: (player_pos + offset).extend(11.0),
                rotation,
                ..default()
            },
        ));
    }

    // While the swing is live, damage enemies in the arc — once each per swing.
    // Damage scales with the player's Strength stat.
    if sword.active > 0.0 {
        let damage = stats.sword_damage();
        let hit_center = if sword.down {
            player_pos + Vec2::new(0.0, -HIT_DOWN_REACH)
        } else {
            player_pos + Vec2::new(sword.dir * HIT_REACH, 0.0)
        };
        let mut connected = false; // anything struck this frame (for the pogo bounce)
        for (entity, enemy_tf, mut enemy) in &mut enemies {
            if sword.hit.contains(&entity) {
                continue;
            }
            let delta = (enemy_tf.translation.truncate() - hit_center).abs();
            if delta.x < HIT_HALF.x + ENEMY_HALF.x && delta.y < HIT_HALF.y + ENEMY_HALF.y {
                enemy.health -= damage;
                sword.hit.insert(entity);
                connected = true;
                sfx.write(PlaySfx(Sfx::EnemyHit));
                info!(
                    "hit enemy kind {} for {} ({} hp left)",
                    enemy.kind, damage, enemy.health
                );
            }
        }

        // The boss is only vulnerable once the fight is underway.
        if fight.locked {
            for (entity, boss_tf, mut boss) in &mut bosses {
                if sword.hit.contains(&entity) {
                    continue;
                }
                let delta = (boss_tf.translation.truncate() - hit_center).abs();
                if delta.x < HIT_HALF.x + BOSS_HALF.x && delta.y < HIT_HALF.y + BOSS_HALF.y {
                    boss.health -= damage;
                    boss.flash = 0.12;
                    sword.hit.insert(entity);
                    connected = true;
                    sfx.write(PlaySfx(Sfx::EnemyHit));
                }
            }
        }

        // Pogo: a downward strike that lands on *anything* (enemy, boss, or a hazard such
        // as a spike/rock) bounces the player up — once per swing — even while invulnerable.
        if sword.down && !sword.pogoed && !jump.grounded() {
            let on_hazard = hazards.iter().any(|(t, h)| {
                let delta = (t.translation.truncate() - hit_center).abs();
                delta.x < HIT_HALF.x + h.half.x && delta.y < HIT_HALF.y + h.half.y
            });
            if connected || on_hazard {
                velocity.0.y = POGO_SPEED;
                jump.start_pogo();
                sword.pogoed = true;
                sfx.write(PlaySfx(Sfx::Jump));
            }
        }
    }
}

fn fade_slashes(time: Res<Time>, mut commands: Commands, mut slashes: Query<(Entity, &mut Slash)>) {
    let dt = time.delta_secs();
    for (entity, mut slash) in &mut slashes {
        slash.0 -= dt;
        if slash.0 <= 0.0 {
            commands.entity(entity).despawn();
        }
    }
}

// --- energy HUD ----------------------------------------------------------

const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);

#[derive(Component)]
struct EnergyHud;

fn spawn_energy_hud(mut commands: Commands, existing: Query<(), With<EnergyHud>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        EnergyHud,
        Text2d::new("Energy: 0"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.6, 0.95, 0.7)),
        Transform::from_xyz(0.0, 0.0, 50.0),
    ));
}

fn despawn_energy_hud(mut commands: Commands, hud: Query<Entity, With<EnergyHud>>) {
    for entity in &hud {
        commands.entity(entity).despawn();
    }
}

#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn update_energy_hud(
    energy: Res<Energy>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    mut hud: Query<(&mut Transform, &mut Text2d), (With<EnergyHud>, Without<Camera2d>)>,
) {
    let Ok((camera_tf, projection)) = camera.single() else {
        return;
    };
    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };
    let top_left = camera_tf.translation.truncate() + Vec2::new(-VIEW_HALF.x, VIEW_HALF.y) * scale;
    for (mut transform, mut text) in &mut hud {
        // Centre-anchored; offset right enough that the label stays on screen.
        let pos = top_left + Vec2::new(66.0, -54.0) * scale;
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        transform.scale = Vec3::splat(scale);
        text.0 = format!("Energy: {}", energy.0);
    }
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Energy>()
            .init_resource::<LostEnergy>()
            .init_resource::<Sword>()
            .add_systems(OnEnter(GameState::Playing), spawn_energy_hud)
            .add_systems(OnExit(GameState::Playing), despawn_energy_hud)
            .add_systems(Update, enemy_ai.in_set(GameSet::Movement))
            .add_systems(
                Update,
                (
                    player_attack,
                    enemy_death,
                    collect_energy,
                    recover_lost_energy,
                    fade_slashes,
                )
                    .chain()
                    .in_set(GameSet::Hazards),
            )
            .add_systems(
                Update,
                update_energy_hud.run_if(in_state(GameState::Playing)),
            );
    }
}
