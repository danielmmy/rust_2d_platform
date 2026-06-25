use bevy::prelude::*;

/// Top-level game flow: main menu, then load assets, then play.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    MainMenu,
    Loading,
    Playing,
    /// The dev-only level builder (only ever entered in debug builds).
    #[cfg(debug_assertions)]
    Editor,
}
