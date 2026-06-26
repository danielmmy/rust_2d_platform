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

use bevy::prelude::*;

use crate::GameSet;
use crate::input::PlayerIntent;
use crate::physics::{self, Solids};
use crate::player::{PLAYER_HALF, Player};
use crate::state::GameState;
use crate::world::{GameAssets, MapEntity};

// --- enemies -------------------------------------------------------------

/// Half-extents of an enemy (collision + contact).
pub(crate) const ENEMY_HALF: Vec2 = Vec2::new(10.0, 13.0);
const ENEMY_HEALTH: i32 = 3;
const ENEMY_SPEED: f32 = 56.0;
const ENEMY_GRAVITY: f32 = 1400.0;
const ENEMY_MAX_FALL: f32 = 800.0;

/// A ground-patrolling enemy: walks `dir`, falling under gravity, and turns at
/// walls and ledge edges.
#[derive(Component)]
pub(crate) struct Enemy {
    health: i32,
    dir: f32,
    vy: f32,
}

impl Enemy {
    /// A fresh enemy walking right.
    pub(crate) fn new() -> Self {
        Self {
            health: ENEMY_HEALTH,
            dir: 1.0,
            vy: 0.0,
        }
    }
}

fn patrol_enemies(
    time: Res<Time>,
    solids: Res<Solids>,
    mut enemies: Query<(&mut Transform, &mut Enemy, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for (mut transform, mut enemy, mut sprite) in &mut enemies {
        enemy.vy = (enemy.vy - ENEMY_GRAVITY * dt).max(-ENEMY_MAX_FALL);
        let mut center = transform.translation.truncate();

        if physics::collide_x(
            &solids,
            &mut center,
            ENEMY_HALF,
            enemy.dir * ENEMY_SPEED * dt,
        ) {
            enemy.dir = -enemy.dir; // hit a wall → turn around
        }
        let (blocked, landed) = physics::collide_y(&solids, &mut center, ENEMY_HALF, enemy.vy * dt);
        if blocked {
            enemy.vy = 0.0;
        }
        // Turn at a ledge: no ground just ahead of the leading foot.
        if landed {
            let ahead = center.x + enemy.dir * (ENEMY_HALF.x + 2.0);
            if !solids.solid_at(ahead, center.y - ENEMY_HALF.y - 2.0) {
                enemy.dir = -enemy.dir;
            }
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
/// How far in front of the player the hitbox centre sits, and its half-extents.
const HIT_REACH: f32 = 22.0;
const HIT_HALF: Vec2 = Vec2::new(16.0, 16.0);
const SWORD_DAMAGE: i32 = 1;
/// How long a slash sprite lingers.
const SLASH_TIME: f32 = 0.12;

/// The player's sword state: which combo step (0 = idle, 1..=3) and its timers.
#[derive(Resource, Default)]
struct Sword {
    step: u8,
    cooldown: f32,
    combo_window: f32,
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
    mut enemies: Query<(&Transform, &mut Enemy), Without<Player>>,
) {
    let dt = time.delta_secs();
    sword.cooldown = (sword.cooldown - dt).max(0.0);
    sword.combo_window = (sword.combo_window - dt).max(0.0);
    if sword.combo_window <= 0.0 {
        sword.step = 0; // window lapsed → combo resets
    }

    if !intent.attack_pressed || sword.cooldown > 0.0 {
        return;
    }
    let Ok((player_tf, sprite)) = player.single() else {
        return;
    };

    // Advance (or restart) the combo and re-arm the timers.
    sword.step = if sword.combo_window > 0.0 && sword.step < 3 {
        sword.step + 1
    } else {
        1
    };
    sword.cooldown = SWING_COOLDOWN;
    sword.combo_window = SWING_COOLDOWN + COMBO_GRACE;

    let facing = if sprite.flip_x { -1.0 } else { 1.0 };
    let player_pos = player_tf.translation.truncate();
    let hit_center = player_pos + Vec2::new(facing * HIT_REACH, 0.0);

    for (enemy_tf, mut enemy) in &mut enemies {
        let delta = (enemy_tf.translation.truncate() - hit_center).abs();
        if delta.x < HIT_HALF.x + ENEMY_HALF.x && delta.y < HIT_HALF.y + ENEMY_HALF.y {
            enemy.health -= SWORD_DAMAGE;
        }
    }

    // Slash visual — bigger and gold on the finisher.
    let (size, color) = match sword.step {
        3 => (Vec2::new(40.0, 48.0), Color::srgb(1.0, 0.85, 0.4)),
        2 => (Vec2::new(34.0, 42.0), Color::srgb(0.85, 0.95, 1.0)),
        _ => (Vec2::new(30.0, 38.0), Color::srgb(0.6, 0.9, 1.0)),
    };
    commands.spawn((
        MapEntity,
        Slash(SLASH_TIME),
        Sprite {
            image: assets.slash.clone(),
            custom_size: Some(size),
            color,
            flip_x: facing < 0.0,
            ..default()
        },
        Transform::from_xyz(hit_center.x, player_pos.y, 11.0),
    ));
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
            .add_systems(Update, patrol_enemies.in_set(GameSet::Movement))
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
