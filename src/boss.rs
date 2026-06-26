//! The boss and the **arena lock**. A room becomes an arena via its map `fog_wall`
//! list (`crate::world::ArenaSpawn`) — the combatants to fight — with `F` glyphs
//! drawing the threshold mist. The list's foes aren't spawned until you **enter** the
//! room; entering then **seals the exits** ([`BossFight`]) until every one of them is
//! dead ([`arena_clear`]). No interaction — crossing in commits you.
//!
//! The boss cycles three attacks ([`Attack`]) — a **slam** leap, a fan of
//! **throwables**, and **summoning** lesser foes — growing more aggressive past half
//! health. Killing it grants a heap of energy and the **double jump** ability
//! ([`crate::player::Abilities`]), both persisted, so a cleared boss never returns.

use std::collections::HashSet;

use bevy::prelude::*;

use crate::GameSet;
use crate::combat::{Enemy, Energy, LostEnergy};
use crate::hazards::Hazard;
use crate::health::Hurt;
use crate::physics::{self, Solids, TILE};
use crate::player::{Abilities, PLAYER_HALF, Player};
use crate::save::Save;
use crate::state::GameState;
use crate::stats::{self, Stats};
use crate::world::{CurrentRoom, GameAssets, MapEntity};

pub(crate) const BOSS_HALF: Vec2 = Vec2::new(38.0, 44.0);
const BOSS_SPEED: f32 = 90.0;
const BOSS_GRAVITY: f32 = 1400.0;
const BOSS_MAX_FALL: f32 = 900.0;
const SLAM_VY: f32 = 540.0;
const SLAM_SPEED: f32 = 170.0;

const IDLE_TIME: f32 = 0.95;
const WINDUP_TIME: f32 = 0.45;
const RECOVER_TIME: f32 = 0.6;

const PROJECTILES: i32 = 3;
const PROJECTILE_SPEED: f32 = 280.0;
const PROJECTILE_HALF: Vec2 = Vec2::new(8.0, 8.0);
const PROJECTILE_LIFE: f32 = 2.6;
const SUMMONS: i32 = 2;
/// Enemy kind the boss summons (the red chaser).
const SUMMON_KIND: usize = 1;

/// Energy granted for beating the boss.
const REWARD_ENERGY: u32 = 80;

/// A boss type: its health and sprite tint. A `fog_wall` entry with `boss: 1` picks one
/// by its `kind` index (the `boss.png` art is shared; `color` multiplies it — `WHITE`
/// leaves it its native purple).
struct BossKind {
    health: i32,
    color: Color,
}

/// The boss types, indexed by a `fog_wall` boss entry's `kind`.
const BOSS_KINDS: [BossKind; 2] = [
    // 0: the original.
    BossKind {
        health: 28,
        color: Color::WHITE,
    },
    // 1: a tougher, red-tinted brute with double the health.
    BossKind {
        health: 56,
        color: Color::srgb(2.6, 0.55, 0.4),
    },
];

/// One of the boss's attacks, chosen in rotation.
#[derive(Clone, Copy)]
enum Attack {
    /// Leap toward the player and crash down.
    Slam,
    /// Throw a fan of projectiles at the player.
    Throw,
    /// Summon lesser enemies into the arena.
    Summon,
}

/// The boss's behaviour state.
enum BossState {
    /// Pacing toward the player between attacks (`timer` until the next one).
    Idle(f32),
    /// Telegraphing `Attack` (`timer` until it fires).
    Windup(f32, Attack),
    /// Mid-slam: airborne, committed to a direction.
    Slamming,
    /// Post-attack pause (`timer`).
    Recover(f32),
}

#[derive(Component)]
pub(crate) struct Boss {
    pub(crate) health: i32,
    max_health: i32,
    /// Base sprite tint (from the boss kind); modulated by the hit flash.
    color: Color,
    dir: f32,
    vy: f32,
    state: BossState,
    next: u8,
    /// Hit-flash timer (set when the sword connects — see [`crate::combat`]).
    pub(crate) flash: f32,
}

impl Boss {
    /// Past half health the boss attacks faster — a crude second phase.
    fn enraged(&self) -> bool {
        self.health * 2 <= self.max_health
    }
    fn idle_time(&self) -> f32 {
        if self.enraged() {
            IDLE_TIME * 0.6
        } else {
            IDLE_TIME
        }
    }
    fn recover_time(&self) -> f32 {
        if self.enraged() {
            RECOVER_TIME * 0.6
        } else {
            RECOVER_TIME
        }
    }
}

/// A fog-wall mist cell (cosmetic; the seal is the locked exits).
#[derive(Component)]
pub(crate) struct FogGate;

/// A thrown projectile that hurts the player on contact.
#[derive(Component)]
struct BossProjectile {
    vel: Vec2,
    life: f32,
}

/// Whether the boss fight is underway. While locked, the arena's exits are sealed
/// (see [`crate::world`]).
#[derive(Resource, Default)]
pub(crate) struct BossFight {
    pub(crate) locked: bool,
    /// Whether the live arena re-arms on a bench rest (set from the room's
    /// `fog_respawn` flag). When `true`, clearing it only records a *transient*
    /// win ([`ClearedArenas`]); when `false`, the win is *permanent*
    /// ([`ClearedBosses`], persisted).
    pub(crate) respawn: bool,
}

/// The rooms whose boss has been beaten (so it doesn't respawn). Synced from the
/// [`Save`] — one entry per cleared arena, so beating one boss doesn't clear another.
#[derive(Resource, Default)]
pub(crate) struct ClearedBosses(pub(crate) HashSet<String>);

/// Arenas cleared **this checkpoint** — every arena lands here when won, but this set
/// is wiped on rest at a bench, so non-boss arenas re-arm (their foes respawn). Not
/// persisted: a fresh session re-arms them all. (Boss arenas also live in
/// [`ClearedBosses`], which a bench rest does *not* clear — bosses stay dead.)
#[derive(Resource, Default)]
pub(crate) struct ClearedArenas(pub(crate) HashSet<String>);

pub struct BossPlugin;

impl Plugin for BossPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BossFight>()
            .init_resource::<ClearedBosses>()
            .init_resource::<ClearedArenas>()
            .add_systems(
                OnEnter(GameState::Playing),
                (apply_boss_save, spawn_boss_hud),
            )
            .add_systems(OnExit(GameState::Playing), despawn_boss_hud)
            .add_systems(Update, boss_ai.in_set(GameSet::Movement))
            .add_systems(
                Update,
                (boss_projectiles, boss_death, arena_clear)
                    .chain()
                    .in_set(GameSet::Hazards),
            )
            .add_systems(Update, update_boss_hud.run_if(in_state(GameState::Playing)));
    }
}

/// Pull the set of already-beaten boss rooms from the save when a game starts, and
/// start the session with every non-boss arena live again.
fn apply_boss_save(
    save: Res<Save>,
    mut cleared: ResMut<ClearedBosses>,
    mut session: ResMut<ClearedArenas>,
) {
    cleared.0 = save
        .cleared_bosses
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    session.0.clear();
}

/// Spawn a boss of `kind` at `pos`, already awake and lethal — it's only spawned into
/// a live, sealed arena. A `MapEntity`, so it clears on reload and re-spawns a fresh
/// attempt.
pub(crate) fn spawn_boss_at(commands: &mut Commands, assets: &GameAssets, kind: usize, pos: Vec2) {
    let spec = &BOSS_KINDS[kind.min(BOSS_KINDS.len() - 1)];
    commands.spawn((
        MapEntity,
        Boss {
            health: spec.health,
            max_health: spec.health,
            color: spec.color,
            dir: -1.0,
            vy: 0.0,
            state: BossState::Idle(IDLE_TIME),
            next: 0,
            flash: 0.0,
        },
        Hazard {
            half: BOSS_HALF * 0.85, // lethal on contact
        },
        Sprite {
            image: assets.boss.clone(),
            color: spec.color,
            custom_size: Some(Vec2::splat(112.0)),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, 4.0),
    ));
}

/// Spawn one fog-wall cell's translucent mist visual at `center` (purely cosmetic —
/// the seal is the locked exits). Despawned by [`arena_clear`] when the room is won.
pub(crate) fn spawn_fog_cell(commands: &mut Commands, center: Vec2) {
    commands.spawn((
        MapEntity,
        FogGate,
        Sprite {
            color: Color::srgba(0.55, 0.4, 0.85, 0.6),
            custom_size: Some(Vec2::splat(TILE)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 6.0),
    ));
}

/// The arena is cleared when every combatant (the boss and all enemies) is dead:
/// unlock the exits and lift the mist. How the win is *recorded* depends on the room's
/// `fog_respawn` flag (mirrored into [`BossFight::respawn`]): a respawning arena lands in
/// the transient [`ClearedArenas`] (re-arms on the next bench rest), while a permanent one
/// is persisted in [`ClearedBosses`] (stays cleared forever). A boss arena is permanent and
/// is *also* recorded by [`boss_death`], so the persist here is guarded to run at most once.
#[allow(clippy::too_many_arguments)] // distinct resources for the two record paths
fn arena_clear(
    mut commands: Commands,
    mut fight: ResMut<BossFight>,
    mut session: ResMut<ClearedArenas>,
    mut cleared: ResMut<ClearedBosses>,
    mut save: ResMut<Save>,
    energy: Res<Energy>,
    stats: Res<Stats>,
    lost: Res<LostEnergy>,
    current: Res<CurrentRoom>,
    bosses: Query<(), With<Boss>>,
    enemies: Query<(), With<Enemy>>,
    fog: Query<Entity, With<FogGate>>,
) {
    if fight.locked && bosses.is_empty() && enemies.is_empty() {
        fight.locked = false;
        if fight.respawn {
            // Transient: wiped on the next bench rest, so the foes respawn.
            session.0.insert(current.name.clone());
        } else if cleared.0.insert(current.name.clone()) {
            // Permanent: persist it (skipped if `boss_death` already recorded this room).
            save.cleared_bosses = cleared.0.iter().cloned().collect::<Vec<_>>().join(",");
            stats::write_progress(&mut save, &energy, &stats, &lost);
        }
        for cell in &fog {
            commands.entity(cell).despawn();
        }
    }
}

/// Drive the boss: gravity, pacing, the attack rotation, and each attack's launch.
/// (Bosses only exist inside a live, locked arena, so it's always active.)
fn boss_ai(
    time: Res<Time>,
    solids: Res<Solids>,
    assets: Res<GameAssets>,
    mut commands: Commands,
    players: Query<&Transform, (With<Player>, Without<Boss>)>,
    mut bosses: Query<(&mut Transform, &mut Boss, &mut Sprite)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let player_pos = players.single().ok().map(|t| t.translation.truncate());

    for (mut transform, mut boss, mut sprite) in &mut bosses {
        boss.flash = (boss.flash - dt).max(0.0);
        let mut center = transform.translation.truncate();

        // Face the player.
        if let Some(pp) = player_pos {
            boss.dir = if pp.x >= center.x { 1.0 } else { -1.0 };
        }

        let grounded = solids.solid_at(center.x, center.y - BOSS_HALF.y - 2.0);
        let mut step = 0.0;

        match boss.state {
            BossState::Idle(mut t) => {
                step = boss.dir * BOSS_SPEED * dt; // amble toward the player
                t -= dt;
                boss.state = if t <= 0.0 {
                    let attack = match boss.next % 3 {
                        0 => Attack::Slam,
                        1 => Attack::Throw,
                        _ => Attack::Summon,
                    };
                    boss.next = boss.next.wrapping_add(1);
                    BossState::Windup(WINDUP_TIME, attack)
                } else {
                    BossState::Idle(t)
                };
            }
            BossState::Windup(mut t, attack) => {
                t -= dt;
                if t <= 0.0 {
                    boss.state = launch(
                        attack,
                        &mut boss,
                        center,
                        player_pos,
                        &assets,
                        &mut commands,
                    );
                } else {
                    boss.state = BossState::Windup(t, attack);
                }
            }
            BossState::Slamming => {
                step = boss.dir * SLAM_SPEED * dt; // committed horizontal drive
                if grounded && boss.vy <= 0.0 {
                    boss.state = BossState::Recover(boss.recover_time());
                }
            }
            BossState::Recover(mut t) => {
                t -= dt;
                boss.state = if t <= 0.0 {
                    BossState::Idle(boss.idle_time())
                } else {
                    BossState::Recover(t)
                };
            }
        }

        // Horizontal move, then gravity + vertical collision (all states fall).
        physics::collide_x(&solids, &mut center, BOSS_HALF, step);
        boss.vy = (boss.vy - BOSS_GRAVITY * dt).max(-BOSS_MAX_FALL);
        let (vblocked, _) = physics::collide_y(&solids, &mut center, BOSS_HALF, boss.vy * dt);
        if vblocked {
            boss.vy = 0.0;
        }
        transform.translation.x = center.x;
        transform.translation.y = center.y;

        // Flip to face the player; flash bright when freshly struck, else the kind tint.
        sprite.flip_x = boss.dir < 0.0;
        sprite.color = if boss.flash > 0.0 {
            Color::srgb(1.8, 1.4, 1.4)
        } else {
            boss.color
        };
    }
}

/// Fire `attack` and return the state to enter afterwards.
fn launch(
    attack: Attack,
    boss: &mut Boss,
    center: Vec2,
    player_pos: Option<Vec2>,
    assets: &GameAssets,
    commands: &mut Commands,
) -> BossState {
    match attack {
        Attack::Slam => {
            boss.vy = SLAM_VY; // leap; horizontal drive happens in `Slamming`
            BossState::Slamming
        }
        Attack::Throw => {
            let target = player_pos.unwrap_or(center + Vec2::new(boss.dir * 100.0, 0.0));
            let base = (target - center).normalize_or_zero();
            for i in 0..PROJECTILES {
                // Fan the shots around the aim direction.
                let spread = (i - PROJECTILES / 2) as f32 * 0.28;
                let (s, c) = spread.sin_cos();
                let dir = Vec2::new(base.x * c - base.y * s, base.x * s + base.y * c);
                commands.spawn((
                    MapEntity,
                    BossProjectile {
                        vel: dir * PROJECTILE_SPEED,
                        life: PROJECTILE_LIFE,
                    },
                    Sprite {
                        image: assets.orb.clone(),
                        custom_size: Some(Vec2::splat(16.0)),
                        color: Color::srgb(1.0, 0.5, 0.4),
                        ..default()
                    },
                    Transform::from_xyz(center.x, center.y, 5.0),
                ));
            }
            BossState::Recover(boss.recover_time())
        }
        Attack::Summon => {
            for i in 0..SUMMONS {
                let offset = (i as f32 - 0.5) * 60.0;
                commands.spawn((
                    MapEntity,
                    Enemy::new(SUMMON_KIND),
                    Hazard {
                        half: Vec2::new(10.0, 13.0),
                    },
                    Sprite {
                        image: assets.enemy_sheets[SUMMON_KIND].clone(),
                        color: crate::combat::ENEMY_KINDS[SUMMON_KIND].color,
                        custom_size: Some(Vec2::splat(16.0)),
                        ..default()
                    },
                    Transform::from_xyz(center.x + offset, center.y, 2.0),
                ));
            }
            BossState::Recover(boss.recover_time())
        }
    }
}

/// Move projectiles; despawn on a wall, on timeout, or after hurting the player.
fn boss_projectiles(
    time: Res<Time>,
    solids: Res<Solids>,
    mut commands: Commands,
    mut hurt: MessageWriter<Hurt>,
    player: Query<&Transform, (With<Player>, Without<BossProjectile>)>,
    mut shots: Query<(Entity, &mut Transform, &mut BossProjectile)>,
) {
    let dt = time.delta_secs();
    let player_pos = player.single().ok().map(|t| t.translation.truncate());
    for (entity, mut transform, mut shot) in &mut shots {
        shot.life -= dt;
        let mut center = transform.translation.truncate();
        center += shot.vel * dt;
        let hit_player = player_pos.is_some_and(|pp| {
            let d = (pp - center).abs();
            d.x < PROJECTILE_HALF.x + PLAYER_HALF.x && d.y < PROJECTILE_HALF.y + PLAYER_HALF.y
        });
        if hit_player {
            hurt.write(Hurt::From(center));
        }
        if shot.life <= 0.0 || hit_player || solids.solid_at(center.x, center.y) {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation.x = center.x;
        transform.translation.y = center.y;
    }
}

/// When the boss is spent: reward the player (energy + double jump), persist it all,
/// and clear the boss along with its shots and summons. ([`arena_clear`] then unlocks
/// the room once nothing else is left alive.)
#[allow(clippy::too_many_arguments, clippy::type_complexity)] // distinct queries/resources
fn boss_death(
    mut commands: Commands,
    mut cleared: ResMut<ClearedBosses>,
    mut energy: ResMut<Energy>,
    mut abilities: ResMut<Abilities>,
    mut save: ResMut<Save>,
    stats: Res<Stats>,
    lost: Res<LostEnergy>,
    current: Res<CurrentRoom>,
    boss: Query<(Entity, &Boss)>,
    debris: Query<Entity, Or<(With<BossProjectile>, With<Enemy>)>>,
) {
    let Ok((entity, boss)) = boss.single() else {
        return;
    };
    if boss.health > 0 {
        return;
    }

    energy.0 += REWARD_ENERGY;
    abilities.double_jump = true;
    save.double_jump = true;
    // Mark *this* arena cleared (so other bosses are unaffected) and persist it all.
    cleared.0.insert(current.name.clone());
    save.cleared_bosses = cleared.0.iter().cloned().collect::<Vec<_>>().join(",");
    stats::write_progress(&mut save, &energy, &stats, &lost);

    commands.entity(entity).despawn();
    for entity in &debris {
        commands.entity(entity).despawn();
    }
}

// --- boss health HUD -----------------------------------------------------

const VIEW_HALF: Vec2 = Vec2::new(480.0, 270.0);
const BOSS_BAR_SIZE: Vec2 = Vec2::new(360.0, 14.0);
const BOSS_BAR_BG: Color = Color::srgb(0.1, 0.06, 0.08);

#[derive(Component)]
struct BossBarBg;
#[derive(Component)]
struct BossBarFill;

fn spawn_boss_hud(mut commands: Commands, existing: Query<(), With<BossBarBg>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        BossBarBg,
        Sprite {
            color: BOSS_BAR_BG,
            custom_size: Some(BOSS_BAR_SIZE),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 50.0),
        Visibility::Hidden,
    ));
    commands.spawn((
        BossBarFill,
        Sprite {
            custom_size: Some(BOSS_BAR_SIZE),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 51.0),
        Visibility::Hidden,
    ));
}

#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn despawn_boss_hud(
    mut commands: Commands,
    bars: Query<Entity, Or<(With<BossBarBg>, With<BossBarFill>)>>,
) {
    for entity in &bars {
        commands.entity(entity).despawn();
    }
}

/// Show a boss health bar across the top while the fight is on; hide it otherwise.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn update_boss_hud(
    fight: Res<BossFight>,
    camera: Query<(&Transform, &Projection), With<Camera2d>>,
    boss: Query<&Boss>,
    mut bg: Query<(&mut Transform, &mut Visibility), (With<BossBarBg>, Without<Camera2d>)>,
    mut fill: Query<
        (&mut Transform, &mut Sprite, &mut Visibility),
        (With<BossBarFill>, Without<BossBarBg>, Without<Camera2d>),
    >,
) {
    // Only while a boss is actually present — a boss-less arena (e.g. an enemy fog wall)
    // locks the exits too, but has no health bar to show.
    let show = fight.locked && !boss.is_empty();
    let (
        Ok((camera_tf, projection)),
        Ok((mut bg_tf, mut bg_vis)),
        Ok((mut fill_tf, mut sprite, mut fill_vis)),
    ) = (camera.single(), bg.single_mut(), fill.single_mut())
    else {
        return;
    };
    if !show {
        *bg_vis = Visibility::Hidden;
        *fill_vis = Visibility::Hidden;
        return;
    }
    *bg_vis = Visibility::Visible;
    *fill_vis = Visibility::Visible;

    let scale = match projection {
        Projection::Orthographic(ortho) => ortho.scale,
        _ => 1.0,
    };
    // Centred near the top of the viewport, scaled to stay a constant on-screen size.
    let top = camera_tf.translation.truncate() + Vec2::new(0.0, VIEW_HALF.y - 24.0) * scale;
    let frac = boss
        .single()
        .ok()
        .map(|b| (b.health.max(0) as f32 / b.max_health as f32).clamp(0.0, 1.0))
        .unwrap_or(0.0);

    bg_tf.translation.x = top.x;
    bg_tf.translation.y = top.y;
    bg_tf.scale = Vec3::splat(scale);

    let width = BOSS_BAR_SIZE.x * frac;
    sprite.custom_size = Some(Vec2::new(width, BOSS_BAR_SIZE.y));
    // Left-aligned within the bar: centre at left edge + half the fill width.
    let left = top.x - BOSS_BAR_SIZE.x * 0.5 * scale;
    fill_tf.translation.x = left + width * 0.5 * scale;
    fill_tf.translation.y = top.y;
    fill_tf.scale = Vec3::splat(scale);
    sprite.color = Color::srgb(0.85, 0.18, 0.22);
}
