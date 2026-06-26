//! Sprite-sheet animation for the player.
//!
//! The player texture is an **N×M grid** of equal frames (`assets/sprites/player.png`
//! is a 4×3 sheet). [`attach_animation`] turns the loaded image into a
//! [`TextureAtlasLayout`] sized from that grid — so dropping in a finer sheet later
//! only needs the same grid, or new [`PLAYER_COLS`]/[`PLAYER_ROWS`].
//! [`animate_player`] flips between a few clips by player state: **idle** (with an
//! occasional blink), **jump**, and a **damage** flash. Add rows + [`Clip`]s to
//! grow it.

use std::time::Duration;

use bevy::prelude::*;

use crate::health::Invuln;
use crate::player::{JumpState, Player};
use crate::state::GameState;
use crate::world::GameAssets;

/// The player sheet's grid: columns × rows of equal frames.
const PLAYER_COLS: u32 = 4;
const PLAYER_ROWS: u32 = 3;

/// One animation: a run of `count` frames from `first`, played at `fps`, looping.
struct Clip {
    first: usize,
    count: usize,
    fps: f32,
}

// Frame layout in the 4×3 sheet: row 0 = idle/blink, row 1 = jump, row 2 = damage.
const IDLE: Clip = Clip {
    first: 0,
    count: 4,
    fps: 5.0,
};
const JUMP: Clip = Clip {
    first: 4,
    count: 2,
    fps: 6.0,
};
const DAMAGE: Clip = Clip {
    first: 8,
    count: 2,
    fps: 12.0,
};

/// The player's atlas layout, built once from the loaded sheet.
#[derive(Resource, Default)]
struct PlayerSheet {
    layout: Option<Handle<TextureAtlasLayout>>,
}

/// Animation state on the player: the current clip, its frame, and a frame timer.
#[derive(Component)]
struct PlayerAnimation {
    clip_first: usize,
    frame: usize,
    timer: Timer,
}

impl Default for PlayerAnimation {
    fn default() -> Self {
        Self {
            clip_first: usize::MAX, // forces a clip change on the first frame
            frame: 0,
            timer: Timer::from_seconds(0.2, TimerMode::Repeating),
        }
    }
}

pub struct AnimationPlugin;

impl Plugin for AnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerSheet>().add_systems(
            Update,
            (attach_animation, animate_player)
                .chain()
                .run_if(in_state(GameState::Playing)),
        );
    }
}

/// Give the player a texture atlas + [`PlayerAnimation`] once its sheet is loaded.
#[allow(clippy::type_complexity)] // a Bevy query filter; clearer inline than aliased
fn attach_animation(
    mut commands: Commands,
    assets: Res<GameAssets>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut sheet: ResMut<PlayerSheet>,
    mut player: Query<(Entity, &mut Sprite), (With<Player>, Without<PlayerAnimation>)>,
) {
    let Ok((entity, mut sprite)) = player.single_mut() else {
        return;
    };
    let layout = match &sheet.layout {
        Some(handle) => handle.clone(),
        None => {
            // Size each frame from the actual image, so a re-drawn sheet of the same
            // grid still imports correctly.
            let Some(image) = images.get(&assets.player) else {
                return; // sheet not loaded yet — retry next frame
            };
            let size = image.size();
            let frame = UVec2::new(size.x / PLAYER_COLS, size.y / PLAYER_ROWS);
            let handle = layouts.add(TextureAtlasLayout::from_grid(
                frame,
                PLAYER_COLS,
                PLAYER_ROWS,
                None,
                None,
            ));
            sheet.layout = Some(handle.clone());
            handle
        }
    };
    sprite.texture_atlas = Some(TextureAtlas { layout, index: 0 });
    commands.entity(entity).insert(PlayerAnimation::default());
}

/// Pick a clip from player state and advance its frames.
fn animate_player(
    time: Res<Time>,
    invuln: Res<Invuln>,
    mut player: Query<(&JumpState, &mut Sprite, &mut PlayerAnimation), With<Player>>,
) {
    let Ok((jump, mut sprite, mut anim)) = player.single_mut() else {
        return;
    };

    let clip = if invuln.0 > 0.0 {
        DAMAGE
    } else if !jump.grounded() {
        JUMP
    } else {
        IDLE
    };

    // On a clip change, restart at its first frame and retime.
    if anim.clip_first != clip.first {
        anim.clip_first = clip.first;
        anim.frame = 0;
        anim.timer
            .set_duration(Duration::from_secs_f32(1.0 / clip.fps));
        anim.timer.reset();
    }

    anim.timer.tick(time.delta());
    if anim.timer.just_finished() {
        anim.frame = (anim.frame + 1) % clip.count;
    }

    if let Some(atlas) = &mut sprite.texture_atlas {
        atlas.index = clip.first + anim.frame;
    }
}
