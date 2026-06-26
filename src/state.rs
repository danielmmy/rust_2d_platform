use bevy::prelude::*;

/// Top-level game flow: main menu, then load assets, then play.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GameState {
    #[default]
    MainMenu,
    Loading,
    Playing,
    /// The level builder, reached from a Builder save (see [`crate::editor`]).
    Editor,
}
