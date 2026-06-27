//! Extensible sprite-sheet animation.
//!
//! The **generic core** — [`SpriteAnimation`] + [`advance_animations`] — drives any
//! entity whose `Sprite` has a texture atlas: a [`Clip`] names a run of frames and
//! how it plays ([`Playback::Loop`] / [`Once`](Playback::Once) /
//! [`Manual`](Playback::Manual)). [`atlas_for`] imports an **N×M** sheet into a
//! cached [`TextureAtlasLayout`], sizing each frame from the image so a re-drawn
//! sheet of the same grid just works.
//!
//! On top sit small per-entity **controllers** that just pick a clip each frame:
//! the player (idle blink / jump arc / damage flash) and portals (idle halo /
//! active while in use). Add a new animated thing by loading a sheet, attaching a
//! [`SpriteAnimation`], and writing a controller that calls [`SpriteAnimation::play`].

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;

use crate::boss::{Boss, BossPose};
use crate::combat::{ENEMY_KINDS, Enemy};
use crate::health::Invuln;
use crate::player::{JumpState, MovementConfig, Player, Velocity};
use crate::state::GameState;
use crate::world::{Bench, GameAssets, Teleporter};

// --- generic core --------------------------------------------------------

/// How a clip's frames advance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Playback {
    /// Cycle frames forever.
    Loop,
    /// Play once and hold the last frame.
    #[allow(dead_code)] // part of the clip API; for future one-shot animations
    Once,
    /// Frame set externally each tick by a controller (e.g. mapped to velocity).
    Manual,
}

/// A run of `count` frames from `first`, advanced per `playback` (timer at `fps`).
#[derive(Clone, Copy)]
pub struct Clip {
    pub first: usize,
    pub count: usize,
    pub fps: f32,
    pub playback: Playback,
}

/// Animation state on an entity: its current clip, frame, and frame timer.
#[derive(Component)]
pub struct SpriteAnimation {
    pub clip: Clip,
    pub frame: usize,
    timer: Timer,
}

impl SpriteAnimation {
    pub fn new(clip: Clip) -> Self {
        Self {
            clip,
            frame: 0,
            timer: Timer::from_seconds(1.0 / clip.fps.max(0.001), TimerMode::Repeating),
        }
    }

    /// Switch to `clip`, restarting from its first frame if it's a different clip.
    pub fn play(&mut self, clip: Clip) {
        if self.clip.first == clip.first {
            return;
        }
        self.clip = clip;
        self.frame = 0;
        self.timer
            .set_duration(Duration::from_secs_f32(1.0 / clip.fps.max(0.001)));
        self.timer.reset();
    }
}

/// Cache of atlas layouts by source image, so each sheet is gridded once.
#[derive(Resource, Default)]
struct AtlasCache(HashMap<AssetId<Image>, Handle<TextureAtlasLayout>>);

/// Get (building + caching if needed) the `cols`×`rows` grid layout for `image`.
/// `None` until the image has loaded.
fn atlas_for(
    cache: &mut AtlasCache,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &Assets<Image>,
    image: &Handle<Image>,
    cols: u32,
    rows: u32,
) -> Option<Handle<TextureAtlasLayout>> {
    if let Some(handle) = cache.0.get(&image.id()) {
        return Some(handle.clone());
    }
    let size = images.get(image)?.size();
    let frame = UVec2::new(size.x / cols.max(1), size.y / rows.max(1));
    let handle = layouts.add(TextureAtlasLayout::from_grid(frame, cols, rows, None, None));
    cache.0.insert(image.id(), handle.clone());
    Some(handle)
}

/// Advance every timer-driven animation and write its current atlas frame.
fn advance_animations(time: Res<Time>, mut query: Query<(&mut SpriteAnimation, &mut Sprite)>) {
    for (mut anim, mut sprite) in &mut query {
        let last = anim.clip.count.saturating_sub(1);
        if anim.clip.playback != Playback::Manual {
            anim.timer.tick(time.delta());
            if anim.timer.just_finished() {
                anim.frame = match anim.clip.playback {
                    Playback::Once => (anim.frame + 1).min(last),
                    _ => (anim.frame + 1) % anim.clip.count.max(1),
                };
            }
        }
        if let Some(atlas) = &mut sprite.texture_atlas {
            atlas.index = anim.clip.first + anim.frame.min(last);
        }
    }
}

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AtlasCache>().add_systems(
            Update,
            (
                attach_player,
                attach_portals,
                attach_benches,
                attach_enemies,
                attach_boss,
                control_player,
                control_portals,
                control_boss,
                advance_animations,
            )
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

// --- player --------------------------------------------------------------

const PLAYER_COLS: u32 = 6;
const PLAYER_ROWS: u32 = 4;
/// Minimum horizontal speed (px/s) before the walk cycle plays.
const WALK_SPEED_MIN: f32 = 12.0;

// Frame rows in the 6×4 sheet: 0 = idle/blink, 1 = walk, 2 = jump, 3 = damage.
// The sprite faces right; `player` movement flips it to face left, Hollow-Knight
// style, so facing follows the walk direction.
const PLAYER_IDLE: Clip = Clip {
    first: 0,
    count: 4,
    fps: 5.0,
    playback: Playback::Loop,
};
const PLAYER_WALK: Clip = Clip {
    first: 6,
    count: 6,
    fps: 10.0,
    playback: Playback::Loop,
};
/// The jump is a single run mapped across the arc (see [`control_player`]).
const PLAYER_JUMP: Clip = Clip {
    first: 12,
    count: 4,
    fps: 6.0,
    playback: Playback::Manual,
};
const PLAYER_DAMAGE: Clip = Clip {
    first: 18,
    count: 4,
    fps: 14.0,
    playback: Playback::Loop,
};

/// Give the player a texture atlas + idle [`SpriteAnimation`] once its sheet loads.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_player(
    mut commands: Commands,
    assets: Res<GameAssets>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<AtlasCache>,
    mut player: Query<(Entity, &mut Sprite), (With<Player>, Without<SpriteAnimation>)>,
) {
    let Ok((entity, mut sprite)) = player.single_mut() else {
        return;
    };
    let Some(layout) = atlas_for(
        &mut cache,
        &mut layouts,
        &images,
        &assets.player,
        PLAYER_COLS,
        PLAYER_ROWS,
    ) else {
        return;
    };
    sprite.texture_atlas = Some(TextureAtlas { layout, index: 0 });
    commands
        .entity(entity)
        .insert(SpriteAnimation::new(PLAYER_IDLE));
}

/// Pick the player's clip from state: damage > jump (airborne) > walk > idle.
fn control_player(
    invuln: Res<Invuln>,
    cfg: Res<MovementConfig>,
    mut player: Query<(&JumpState, &Velocity, &mut SpriteAnimation), With<Player>>,
) {
    let Ok((jump, velocity, mut anim)) = player.single_mut() else {
        return;
    };

    if invuln.0 > 0.0 {
        anim.play(PLAYER_DAMAGE);
    } else if !jump.grounded() {
        anim.play(PLAYER_JUMP);
        // Manual: a single run across the arc — launch (frame 0) → apex (mid) → max
        // fall (last), held — keyed to velocity so it follows any jump height.
        let denom = if velocity.0.y >= 0.0 {
            cfg.jump_speed
        } else {
            cfg.max_fall
        };
        let progress = (0.5 - 0.5 * velocity.0.y / denom).clamp(0.0, 1.0);
        let last = anim.clip.count.saturating_sub(1);
        anim.frame = ((progress * anim.clip.count as f32) as usize).min(last);
    } else if velocity.0.x.abs() > WALK_SPEED_MIN {
        anim.play(PLAYER_WALK);
    } else {
        anim.play(PLAYER_IDLE);
    }
}

// --- portals -------------------------------------------------------------

const PORTAL_COLS: u32 = 6;
const PORTAL_ROWS: u32 = 2;

// Frame rows in the 6×2 sheet: 0 = idle halo, 1 = active.
const PORTAL_IDLE: Clip = Clip {
    first: 0,
    count: 6,
    fps: 8.0,
    playback: Playback::Loop,
};
const PORTAL_ACTIVE: Clip = Clip {
    first: 6,
    count: 6,
    fps: 16.0,
    playback: Playback::Loop,
};

/// A portal flares "active" while the player is within this distance of its centre.
const PORTAL_ACTIVE_DIST: f32 = 28.0;

/// Give each portal a texture atlas + idle [`SpriteAnimation`] once its sheet loads.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_portals(
    mut commands: Commands,
    assets: Res<GameAssets>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<AtlasCache>,
    mut portals: Query<(Entity, &mut Sprite), (With<Teleporter>, Without<SpriteAnimation>)>,
) {
    if portals.is_empty() {
        return;
    }
    let Some(layout) = atlas_for(
        &mut cache,
        &mut layouts,
        &images,
        &assets.portal,
        PORTAL_COLS,
        PORTAL_ROWS,
    ) else {
        return;
    };
    for (entity, mut sprite) in &mut portals {
        sprite.texture_atlas = Some(TextureAtlas {
            layout: layout.clone(),
            index: 0,
        });
        commands
            .entity(entity)
            .insert(SpriteAnimation::new(PORTAL_IDLE));
    }
}

/// Portals play their active clip while the player is on/near them, else idle.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn control_portals(
    player: Query<&Transform, With<Player>>,
    mut portals: Query<(&Transform, &mut SpriteAnimation), (With<Teleporter>, Without<Player>)>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();
    for (transform, mut anim) in &mut portals {
        let active = transform.translation.truncate().distance(player_pos) < PORTAL_ACTIVE_DIST;
        anim.play(if active { PORTAL_ACTIVE } else { PORTAL_IDLE });
    }
}

// --- benches -------------------------------------------------------------

const BENCH_COLS: u32 = 6;
const BENCH_ROWS: u32 = 1;

/// The bench seat is static; this clip just drifts its fairy lights. It only loops
/// (no controller needed), so the generic [`advance_animations`] does the rest.
const BENCH_IDLE: Clip = Clip {
    first: 0,
    count: 6,
    fps: 6.0,
    playback: Playback::Loop,
};

/// Give each bench a texture atlas + its looping fairy-light animation once loaded.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_benches(
    mut commands: Commands,
    assets: Res<GameAssets>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<AtlasCache>,
    mut benches: Query<(Entity, &mut Sprite), (With<Bench>, Without<SpriteAnimation>)>,
) {
    if benches.is_empty() {
        return;
    }
    let Some(layout) = atlas_for(
        &mut cache,
        &mut layouts,
        &images,
        &assets.bench,
        BENCH_COLS,
        BENCH_ROWS,
    ) else {
        return;
    };
    for (entity, mut sprite) in &mut benches {
        sprite.texture_atlas = Some(TextureAtlas {
            layout: layout.clone(),
            index: 0,
        });
        commands
            .entity(entity)
            .insert(SpriteAnimation::new(BENCH_IDLE));
    }
}

// --- enemies -------------------------------------------------------------

/// Give each enemy the atlas + animation its [`kind`](Enemy::kind) declares (its
/// sheet's grid and clip), once the sheet is loaded. The kind's sprite sheet is
/// already on the enemy's `Sprite` (set by `world`); we only grid it and animate.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_enemies(
    mut commands: Commands,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<AtlasCache>,
    mut enemies: Query<(Entity, &Enemy, &mut Sprite), Without<SpriteAnimation>>,
) {
    for (entity, enemy, mut sprite) in &mut enemies {
        let kind = &ENEMY_KINDS[enemy.kind];
        let image = sprite.image.clone();
        let Some(layout) = atlas_for(
            &mut cache,
            &mut layouts,
            &images,
            &image,
            kind.cols,
            kind.rows,
        ) else {
            continue; // sheet not loaded yet — retry next frame
        };
        sprite.texture_atlas = Some(TextureAtlas { layout, index: 0 });
        commands
            .entity(entity)
            .insert(SpriteAnimation::new(kind.clip));
    }
}

// --- boss ----------------------------------------------------------------

const BOSS_COLS: u32 = 8;
const BOSS_ROWS: u32 = 6;

// Rows in the 8×6 demon sheet (drawn facing right; `boss` flips it to face the player):
// 0 idle/hover, 1 advance, 2 slam (windup 16-21, air 22-23), 3 throw windup,
// 4 summon windup, 5 recover. The windups are Manual so the telegraph tracks the
// wind-up timer exactly (see `control_boss`).
const BOSS_IDLE: Clip = Clip {
    first: 0,
    count: 6,
    fps: 6.0,
    playback: Playback::Loop,
};
const BOSS_WALK: Clip = Clip {
    first: 8,
    count: 8,
    fps: 11.0,
    playback: Playback::Loop,
};
const BOSS_SLAM_WINDUP: Clip = Clip {
    first: 16,
    count: 6,
    fps: 12.0,
    playback: Playback::Manual,
};
const BOSS_SLAM_AIR: Clip = Clip {
    first: 22,
    count: 2,
    fps: 8.0,
    playback: Playback::Loop,
};
const BOSS_THROW_WINDUP: Clip = Clip {
    first: 24,
    count: 8,
    fps: 12.0,
    playback: Playback::Manual,
};
const BOSS_SUMMON_WINDUP: Clip = Clip {
    first: 32,
    count: 6,
    fps: 12.0,
    playback: Playback::Manual,
};
const BOSS_RECOVER: Clip = Clip {
    first: 40,
    count: 4,
    fps: 6.0,
    playback: Playback::Loop,
};

/// Give the boss its atlas + animation once the sheet loads (the sheet is already on
/// its `Sprite`, set by `boss::spawn_boss_at`).
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_boss(
    mut commands: Commands,
    assets: Res<GameAssets>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut cache: ResMut<AtlasCache>,
    mut bosses: Query<(Entity, &mut Sprite), (With<Boss>, Without<SpriteAnimation>)>,
) {
    for (entity, mut sprite) in &mut bosses {
        let Some(layout) = atlas_for(
            &mut cache,
            &mut layouts,
            &images,
            &assets.boss,
            BOSS_COLS,
            BOSS_ROWS,
        ) else {
            continue;
        };
        sprite.texture_atlas = Some(TextureAtlas { layout, index: 0 });
        commands
            .entity(entity)
            .insert(SpriteAnimation::new(BOSS_IDLE));
    }
}

/// Pick the boss's clip from its [`BossPose`]. Wind-ups are Manual, their frame keyed
/// to the attack's progress so the telegraph builds smoothly and finishes right as the
/// attack fires.
fn control_boss(mut bosses: Query<(&Boss, &mut SpriteAnimation)>) {
    for (boss, mut anim) in &mut bosses {
        match boss.pose() {
            BossPose::Advance => anim.play(BOSS_WALK),
            BossPose::Slam => anim.play(BOSS_SLAM_AIR),
            BossPose::Recover => anim.play(BOSS_RECOVER),
            BossPose::SlamWindup(p) => {
                anim.play(BOSS_SLAM_WINDUP);
                set_manual_frame(&mut anim, p);
            }
            BossPose::ThrowWindup(p) => {
                anim.play(BOSS_THROW_WINDUP);
                set_manual_frame(&mut anim, p);
            }
            BossPose::SummonWindup(p) => {
                anim.play(BOSS_SUMMON_WINDUP);
                set_manual_frame(&mut anim, p);
            }
        }
    }
}

/// Map a `0..1` progress onto a Manual clip's frames.
fn set_manual_frame(anim: &mut SpriteAnimation, progress: f32) {
    let last = anim.clip.count.saturating_sub(1);
    anim.frame = ((progress * anim.clip.count as f32) as usize).min(last);
}
