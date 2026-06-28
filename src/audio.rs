//! Sound effects — small embedded OGG clips, played on game events.
//!
//! Every effect ships in `assets/sounds/` (synthesised by `tools/gen_sfx.py`) and is baked
//! into the binary (see `build.rs`), so a release exe stays self-contained. [`load_sounds`]
//! turns the embedded bytes into [`AudioSource`] handles at startup; gameplay systems fire a
//! [`PlaySfx`] message and [`play_sfx`] spawns a one-shot, self-despawning audio player for
//! it. Per-effect loudness is baked into the synthesis, so playback is uniform here.

use std::collections::HashMap;

use bevy::audio::AudioSource;
use bevy::prelude::*;

use crate::world::EMBEDDED_SOUNDS;

/// One sound effect. Each variant maps to `assets/sounds/<file>.ogg` (see [`Sfx::ALL`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    Footstep,
    Jump,
    DoubleJump,
    WallJump,
    Land,
    Slash,
    SlashHeavy,
    EnemyHit,
    Hurt,
    Pickup,
}

impl Sfx {
    /// Every effect paired with its embedded file name (without the `.ogg`).
    const ALL: [(Sfx, &'static str); 10] = [
        (Sfx::Footstep, "footstep"),
        (Sfx::Jump, "jump"),
        (Sfx::DoubleJump, "double_jump"),
        (Sfx::WallJump, "wall_jump"),
        (Sfx::Land, "land"),
        (Sfx::Slash, "slash"),
        (Sfx::SlashHeavy, "slash_heavy"),
        (Sfx::EnemyHit, "enemy_hit"),
        (Sfx::Hurt, "hurt"),
        (Sfx::Pickup, "pickup"),
    ];
}

/// Request a one-shot sound effect. Written by gameplay systems (jumps, slashes, hits, …).
#[derive(Message, Clone, Copy)]
pub struct PlaySfx(pub Sfx);

/// The decoded clip handles, keyed by [`Sfx`].
#[derive(Resource, Default)]
struct Sounds(HashMap<Sfx, Handle<AudioSource>>);

pub struct AudioFxPlugin;

impl Plugin for AudioFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .init_resource::<Sounds>()
            .add_systems(Startup, load_sounds)
            .add_systems(Update, play_sfx);
    }
}

/// Wrap each embedded OGG in an [`AudioSource`] handle at startup (rodio decodes on play).
fn load_sounds(mut sounds: ResMut<Sounds>, mut sources: ResMut<Assets<AudioSource>>) {
    for (sfx, file) in Sfx::ALL {
        let name = format!("{file}.ogg");
        let bytes = EMBEDDED_SOUNDS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| *b)
            .unwrap_or_else(|| panic!("embedded sound '{name}' not found"));
        let handle = sources.add(AudioSource {
            bytes: bytes.into(),
        });
        sounds.0.insert(sfx, handle);
    }
}

/// Spawn a self-despawning one-shot player for each effect requested this frame.
fn play_sfx(mut commands: Commands, mut events: MessageReader<PlaySfx>, sounds: Res<Sounds>) {
    for &PlaySfx(sfx) in events.read() {
        if let Some(handle) = sounds.0.get(&sfx) {
            commands.spawn((AudioPlayer::new(handle.clone()), PlaybackSettings::DESPAWN));
        }
    }
}
