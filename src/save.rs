//! A tiny three-slot save system.
//!
//! A save records the room to resume in and the last **bench** (the checkpoint you
//! return to on death). Files live under `saves/` as RON, read and written with our
//! own [`crate::ron`] reader — no extra dependency. The title screen picks a slot
//! for a New Game and lists slots to Load.

use bevy::prelude::*;

use crate::ron;

/// Number of save slots offered on the title screen.
pub const SLOTS: usize = 3;

/// The active game's persisted state. `bench_room` empty means no bench has been
/// rested at yet, so death returns the player to the start room.
#[derive(Resource, Default, Clone)]
pub struct Save {
    pub slot: usize,
    /// Player-chosen name for the save (shown in the slot picker; may be empty).
    pub name: String,
    pub room: String,
    pub bench_room: String,
    pub bench_col: i32,
    pub bench_row: i32,
}

const SAVES_DIR: &str = "saves";

fn slot_path(slot: usize) -> String {
    format!("{SAVES_DIR}/slot{slot}.save.ron")
}

impl Save {
    fn to_ron(&self) -> String {
        format!(
            "(name: \"{}\", room: \"{}\", bench_room: \"{}\", bench_col: {}, bench_row: {})\n",
            self.name, self.room, self.bench_room, self.bench_col, self.bench_row
        )
    }

    fn from_ron(slot: usize, text: &str) -> Option<Save> {
        let value = ron::from_str(text).ok()?;
        let opt_str = |name: &str| {
            value
                .try_field(name)
                .and_then(|v| v.as_str().ok())
                .unwrap_or("")
                .to_string()
        };
        let opt_i32 = |name: &str| {
            value
                .try_field(name)
                .and_then(|v| v.as_i32().ok())
                .unwrap_or(0)
        };
        Some(Save {
            slot,
            name: opt_str("name"),
            room: value.field("room").ok()?.as_str().ok()?.to_string(),
            bench_room: opt_str("bench_room"),
            bench_col: opt_i32("bench_col"),
            bench_row: opt_i32("bench_row"),
        })
    }

    /// Has the player rested at a bench (so death returns there)?
    pub fn has_bench(&self) -> bool {
        !self.bench_room.is_empty()
    }
}

/// Read a slot from disk, if it exists and parses.
pub fn read_slot(slot: usize) -> Option<Save> {
    let text = std::fs::read_to_string(slot_path(slot)).ok()?;
    Save::from_ron(slot, &text)
}

/// Write the active save to its slot file (creating `saves/` if needed).
pub fn write_save(save: &Save) {
    let _ = std::fs::create_dir_all(SAVES_DIR);
    let _ = std::fs::write(slot_path(save.slot), save.to_ron());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trips_through_ron() {
        let save = Save {
            slot: 2,
            name: "Wisp".to_string(),
            room: "r1_0".to_string(),
            bench_room: "r1_0".to_string(),
            bench_col: 6,
            bench_row: 20,
        };
        let parsed = Save::from_ron(2, &save.to_ron()).expect("parse");
        assert_eq!(parsed.name, "Wisp");
        assert_eq!(parsed.room, "r1_0");
        assert_eq!(parsed.bench_room, "r1_0");
        assert_eq!(parsed.bench_col, 6);
        assert_eq!(parsed.bench_row, 20);
        assert!(parsed.has_bench());
    }

    #[test]
    fn fresh_save_has_no_bench() {
        let save = Save {
            slot: 0,
            room: "r0_0".to_string(),
            ..default()
        };
        let parsed = Save::from_ron(0, &save.to_ron()).expect("parse");
        assert!(!parsed.has_bench());
    }
}
