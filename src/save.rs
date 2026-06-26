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
///
/// Progression (energy, stat levels) and a dropped **bloodstain** (`lost_*`) ride
/// along too; they're banked at save points (resting, upgrading, dying). A stat
/// level of `0` means "never written" and is read back as the base level 1.
#[derive(Resource, Default, Clone)]
pub struct Save {
    pub slot: usize,
    /// Player-chosen name for the save (shown in the slot picker; may be empty).
    pub name: String,
    pub room: String,
    pub bench_room: String,
    pub bench_col: i32,
    pub bench_row: i32,
    /// Banked energy (the upgrade currency).
    pub energy: u32,
    /// Character stat levels (base 1; see [`crate::stats`]).
    pub vitality: u32,
    pub strength: u32,
    pub poise: u32,
    /// A dropped bloodstain: `lost_amount` energy waiting in `lost_room` at
    /// (`lost_x`, `lost_y`). `lost_amount == 0` means none is pending.
    pub lost_amount: u32,
    pub lost_room: String,
    pub lost_x: f32,
    pub lost_y: f32,
}

const SAVES_DIR: &str = "saves";

fn slot_path(slot: usize) -> String {
    format!("{SAVES_DIR}/slot{slot}.save.ron")
}

impl Save {
    fn to_ron(&self) -> String {
        format!(
            "(name: \"{}\", room: \"{}\", bench_room: \"{}\", bench_col: {}, bench_row: {}, \
             energy: {}, vitality: {}, strength: {}, poise: {}, \
             lost_amount: {}, lost_room: \"{}\", lost_x: {}, lost_y: {})\n",
            self.name,
            self.room,
            self.bench_room,
            self.bench_col,
            self.bench_row,
            self.energy,
            self.vitality,
            self.strength,
            self.poise,
            self.lost_amount,
            self.lost_room,
            self.lost_x,
            self.lost_y,
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
        let opt_u32 = |name: &str| opt_i32(name).max(0) as u32;
        let opt_f32 = |name: &str| {
            value
                .try_field(name)
                .and_then(|v| v.as_f32().ok())
                .unwrap_or(0.0)
        };
        Some(Save {
            slot,
            name: opt_str("name"),
            room: value.field("room").ok()?.as_str().ok()?.to_string(),
            bench_room: opt_str("bench_room"),
            bench_col: opt_i32("bench_col"),
            bench_row: opt_i32("bench_row"),
            energy: opt_u32("energy"),
            vitality: opt_u32("vitality"),
            strength: opt_u32("strength"),
            poise: opt_u32("poise"),
            lost_amount: opt_u32("lost_amount"),
            lost_room: opt_str("lost_room"),
            lost_x: opt_f32("lost_x"),
            lost_y: opt_f32("lost_y"),
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
            energy: 42,
            vitality: 4,
            strength: 3,
            poise: 2,
            lost_amount: 17,
            lost_room: "r0_0".to_string(),
            lost_x: 128.5,
            lost_y: -64.0,
        };
        let parsed = Save::from_ron(2, &save.to_ron()).expect("parse");
        assert_eq!(parsed.name, "Wisp");
        assert_eq!(parsed.room, "r1_0");
        assert_eq!(parsed.bench_room, "r1_0");
        assert_eq!(parsed.bench_col, 6);
        assert_eq!(parsed.bench_row, 20);
        assert_eq!(parsed.energy, 42);
        assert_eq!(parsed.vitality, 4);
        assert_eq!(parsed.strength, 3);
        assert_eq!(parsed.poise, 2);
        assert_eq!(parsed.lost_amount, 17);
        assert_eq!(parsed.lost_room, "r0_0");
        assert_eq!(parsed.lost_x, 128.5);
        assert_eq!(parsed.lost_y, -64.0);
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
