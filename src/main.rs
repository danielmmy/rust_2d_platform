//! A tiny 2D Metroidvania in Bevy 0.19.
//!
//! Three large, interconnected, data-driven maps with platform challenges and
//! environmental dangers (ground spikes, falling rocks, death pits); keyboard
//! **and** gamepad input; and a responsive jump (coyote time, jump buffering,
//! variable height, asymmetric gravity). Built as small Bevy plugins so it's easy
//! to extend, with art and levels under `assets/` that are simple to replace.

mod anim;
mod camera;
#[cfg(debug_assertions)]
mod editor;
mod hazards;
mod health;
mod input;
mod menu;
mod physics;
mod player;
mod ron;
mod save;
mod state;
mod world;
mod worldmap;

use bevy::prelude::*;
use bevy::window::WindowResolution;

use menu::Paused;
use save::Save;
use state::GameState;
use worldmap::MapView;

/// Per-frame ordering of the gameplay systems.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameSet {
    Input,
    Movement,
    Hazards,
    Transitions,
    Camera,
}

fn main() {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Wisp — a tiny Metroidvania".into(),
                    resolution: WindowResolution::new(960, 540),
                    ..default()
                }),
                ..default()
            })
            // Crisp pixel art (nearest-neighbour sampling).
            .set(ImagePlugin::default_nearest()),
    )
    .insert_resource(ClearColor(Color::srgb(0.07, 0.08, 0.12)))
    .init_resource::<Save>()
    .init_state::<GameState>()
    .configure_sets(
        Update,
        (
            GameSet::Input,
            GameSet::Movement,
            GameSet::Hazards,
            GameSet::Transitions,
            GameSet::Camera,
        )
            .chain()
            // Frozen while the world map or the pause menu is open.
            .run_if(
                in_state(GameState::Playing)
                    .and_then(in_state(MapView::Closed))
                    .and_then(in_state(Paused::Running)),
            ),
    )
    .add_plugins((
        input::InputPlugin,
        world::WorldPlugin,
        player::PlayerPlugin,
        hazards::HazardPlugin,
        health::HealthPlugin,
        anim::AnimationPlugin,
        camera::CameraPlugin,
        worldmap::WorldMapPlugin,
        menu::MenuPlugin,
    ));

    // The level builder ships only in debug builds (e.g. `make game-run`).
    #[cfg(debug_assertions)]
    app.add_plugins(editor::EditorPlugin);

    app.run();
}
