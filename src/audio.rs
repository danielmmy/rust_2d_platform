//! Sound effects + background music — small embedded OGG clips.
//!
//! **SFX** (`assets/sounds/`, synthesised by `tools/gen_sfx.py`) play one-shot: gameplay
//! systems fire a [`PlaySfx`] message and [`play_sfx`] spawns a self-despawning player at the
//! current **FX volume**. **Music** (`assets/music/`, one looping track per theme set) follows
//! the room: [`update_music`] watches [`CurrentRoom`] and crossfades by swapping the looping
//! player when the room's [`music`](crate::world::MapData::music) track changes, at the current
//! **Music volume**. Both volumes live in [`Settings`] (the Options menu); everything is baked
//! into the binary (see `build.rs`), so a release stays self-contained.

use std::collections::HashMap;

use bevy::audio::{AudioSink, AudioSource, Volume};
use bevy::prelude::*;

use crate::save::Settings;
use crate::state::GameState;
use crate::world::{CurrentRoom, EMBEDDED_MUSIC, EMBEDDED_SOUNDS};

// --- sound effects -------------------------------------------------------

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
    /// Save/rest jingle (resting at a bench saves & restores).
    Save,
}

impl Sfx {
    /// Every effect paired with its embedded file name (without the `.ogg`).
    const ALL: [(Sfx, &'static str); 11] = [
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
        (Sfx::Save, "save"),
    ];
}

/// Request a one-shot sound effect. Written by gameplay systems (jumps, slashes, hits, …).
#[derive(Message, Clone, Copy)]
pub struct PlaySfx(pub Sfx);

/// The decoded SFX clip handles, keyed by [`Sfx`].
#[derive(Resource, Default)]
struct Sounds(HashMap<Sfx, Handle<AudioSource>>);

// --- music ---------------------------------------------------------------

/// The decoded music loops, keyed by track name (a theme set, e.g. `"deep_caves"`).
#[derive(Resource, Default)]
struct Tracks(HashMap<String, Handle<AudioSource>>);

/// What's playing now — the track name and its entity, so a same-room reload (e.g. resting at
/// a bench) doesn't restart the music.
#[derive(Resource, Default)]
struct NowPlaying {
    track: String,
    entity: Option<Entity>,
}

/// Marks the single looping background-music entity.
#[derive(Component)]
struct MusicTrack;

pub struct AudioFxPlugin;

impl Plugin for AudioFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PlaySfx>()
            .init_resource::<Sounds>()
            .init_resource::<Tracks>()
            .init_resource::<NowPlaying>()
            .add_systems(Startup, load_audio)
            .add_systems(OnExit(GameState::Playing), stop_music)
            .add_systems(
                Update,
                (
                    play_sfx,
                    update_music.run_if(resource_changed::<CurrentRoom>),
                    apply_music_volume.run_if(resource_changed::<Settings>),
                ),
            );
    }
}

/// Wrap each embedded OGG (SFX + music) in an [`AudioSource`] handle at startup; rodio decodes
/// on play.
fn load_audio(
    mut sounds: ResMut<Sounds>,
    mut tracks: ResMut<Tracks>,
    mut sources: ResMut<Assets<AudioSource>>,
) {
    for (sfx, file) in Sfx::ALL {
        let name = format!("{file}.ogg");
        let bytes = EMBEDDED_SOUNDS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, b)| *b)
            .unwrap_or_else(|| panic!("embedded sound '{name}' not found"));
        sounds.0.insert(
            sfx,
            sources.add(AudioSource {
                bytes: bytes.into(),
            }),
        );
    }
    for (name, bytes) in EMBEDDED_MUSIC {
        let stem = name.strip_suffix(".ogg").unwrap_or(name).to_string();
        tracks.0.insert(
            stem,
            sources.add(AudioSource {
                bytes: (*bytes).into(),
            }),
        );
    }
}

/// Spawn a self-despawning one-shot player for each effect requested this frame.
fn play_sfx(
    mut commands: Commands,
    mut events: MessageReader<PlaySfx>,
    sounds: Res<Sounds>,
    settings: Res<Settings>,
) {
    for &PlaySfx(sfx) in events.read() {
        if let Some(handle) = sounds.0.get(&sfx) {
            commands.spawn((
                AudioPlayer::new(handle.clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(settings.fx_volume)),
            ));
        }
    }
}

/// Swap the looping music when the room's track changes (and do nothing on a same-track reload).
fn update_music(
    mut commands: Commands,
    current: Res<CurrentRoom>,
    settings: Res<Settings>,
    tracks: Res<Tracks>,
    mut now: ResMut<NowPlaying>,
) {
    if current.music == now.track {
        return;
    }
    if let Some(entity) = now.entity.take() {
        commands.entity(entity).despawn();
    }
    now.track = current.music.clone();
    if let Some(handle) = tracks.0.get(&current.music) {
        now.entity = Some(
            commands
                .spawn((
                    MusicTrack,
                    AudioPlayer::new(handle.clone()),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(settings.music_volume)),
                ))
                .id(),
        );
    }
}

/// Re-apply the music volume to the playing loop when the setting changes.
fn apply_music_volume(settings: Res<Settings>, mut sinks: Query<&mut AudioSink, With<MusicTrack>>) {
    for mut sink in &mut sinks {
        sink.set_volume(Volume::Linear(settings.music_volume));
    }
}

/// Stop the music when leaving gameplay (so menus/the editor are quiet; it restarts on the
/// next room load).
fn stop_music(mut commands: Commands, mut now: ResMut<NowPlaying>) {
    if let Some(entity) = now.entity.take() {
        commands.entity(entity).despawn();
    }
    now.track.clear();
}
