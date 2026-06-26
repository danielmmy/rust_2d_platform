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
use crate::input::PlayerIntent;
use crate::physics::{self, Solids};
use crate::player::{PLAYER_HALF, Player};
use crate::state::GameState;
use crate::world::{GameAssets, MapEntity};

// --- enemies -------------------------------------------------------------

/// Half-extents of an enemy (collision + contact).
pub(crate) const ENEMY_HALF: Vec2 = Vec2::new(10.0, 13.0);
const ENEMY_GRAVITY: f32 = 1400.0;
const ENEMY_MAX_FALL: f32 = 800.0;

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
pub(crate) const ENEMY_KINDS: [EnemyKind; 4] = [
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
    /// A fresh enemy of `kind`, walking right.
    pub(crate) fn new(kind: usize) -> Self {
        let kind = kind.min(ENEMY_KINDS.len() - 1);
        Self {
            kind,
            health: ENEMY_KINDS[kind].health,
            dir: 1.0,
            vy: 0.0,
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
        let grounded = solids.solid_at(center.x, center.y - ENEMY_HALF.y - 2.0);
        enemy.jump_cd = (enemy.jump_cd - dt).max(0.0);

        // Is the player within this kind's aggro range? If so, face them.
        let engaged = |enemy: &mut Enemy| -> bool {
            let aggro = match kind.ai {
                EnemyAi::Chase { aggro } | EnemyAi::Pounce { aggro, .. } => aggro,
                EnemyAi::Patrol => return false,
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

/// An energy pickup dropped by a dead enemy.
#[derive(Component)]
struct EnergyOrb;

/// How much energy the player has gathered (persists across rooms and deaths).
#[derive(Resource, Default)]
pub struct Energy(pub u32);

fn collect_energy(
    mut commands: Commands,
    mut energy: ResMut<Energy>,
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
const SWORD_DAMAGE: i32 = 1;
/// How long a slash sprite lingers.
const SLASH_TIME: f32 = 0.12;

/// The player's sword state: combo step (0 = idle, 1..=3), its timers, and — while a
/// swing is live — its direction and which enemies it has already hit.
#[derive(Resource, Default)]
struct Sword {
    step: u8,
    cooldown: f32,
    combo_window: f32,
    active: f32,
    dir: f32,
    hit: HashSet<Entity>,
}

/// A short-lived slash effect sprite.
#[derive(Component)]
struct Slash(f32);

fn player_attack(
    time: Res<Time>,
    intent: Res<PlayerIntent>,
    assets: Res<GameAssets>,
    mut sword: ResMut<Sword>,
    mut commands: Commands,
    player: Query<(&Transform, &Sprite), With<Player>>,
    mut enemies: Query<(Entity, &Transform, &mut Enemy), Without<Player>>,
) {
    let dt = time.delta_secs();
    sword.cooldown = (sword.cooldown - dt).max(0.0);
    sword.combo_window = (sword.combo_window - dt).max(0.0);
    sword.active = (sword.active - dt).max(0.0);
    if sword.combo_window <= 0.0 {
        sword.step = 0; // window lapsed → combo resets
    }

    let Ok((player_tf, sprite)) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Start a swing on press (when off cooldown): advance the combo, re-arm timers,
    // open the hitbox window, lock its direction, and flash a slash.
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
        sword.hit.clear();

        // Slash visual — roughly matches the hitbox; bigger and gold on the finisher.
        let (size, color) = match sword.step {
            3 => (Vec2::new(58.0, 58.0), Color::srgb(1.0, 0.85, 0.4)),
            2 => (Vec2::new(52.0, 52.0), Color::srgb(0.85, 0.95, 1.0)),
            _ => (Vec2::new(46.0, 48.0), Color::srgb(0.6, 0.9, 1.0)),
        };
        commands.spawn((
            MapEntity,
            Slash(SLASH_TIME),
            Sprite {
                image: assets.slash.clone(),
                custom_size: Some(size),
                color,
                flip_x: sword.dir < 0.0,
                ..default()
            },
            Transform::from_xyz(player_pos.x + sword.dir * HIT_REACH, player_pos.y, 11.0),
        ));
    }

    // While the swing is live, damage enemies in the arc — once each per swing.
    if sword.active > 0.0 {
        let hit_center = player_pos + Vec2::new(sword.dir * HIT_REACH, 0.0);
        for (entity, enemy_tf, mut enemy) in &mut enemies {
            if sword.hit.contains(&entity) {
                continue;
            }
            let delta = (enemy_tf.translation.truncate() - hit_center).abs();
            if delta.x < HIT_HALF.x + ENEMY_HALF.x && delta.y < HIT_HALF.y + ENEMY_HALF.y {
                enemy.health -= SWORD_DAMAGE;
                sword.hit.insert(entity);
                info!(
                    "hit enemy kind {} for {} ({} hp left)",
                    enemy.kind, SWORD_DAMAGE, enemy.health
                );
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
            .init_resource::<Sword>()
            .add_systems(OnEnter(GameState::Playing), spawn_energy_hud)
            .add_systems(OnExit(GameState::Playing), despawn_energy_hud)
            .add_systems(Update, enemy_ai.in_set(GameSet::Movement))
            .add_systems(
                Update,
                (player_attack, enemy_death, collect_energy, fade_slashes)
                    .chain()
                    .in_set(GameSet::Hazards),
            )
            .add_systems(
                Update,
                update_energy_hud.run_if(in_state(GameState::Playing)),
            );
    }
}
