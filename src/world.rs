//! Maps and the interconnected world.
//!
//! Maps are plain data ([`MapData`]) loaded from `assets/maps/*.map.ron` by our
//! own RON reader (see [`crate::ron`]), so adding a level is just dropping a new
//! file. Each map is an ASCII grid plus a legend (solid / spike / rock / start
//! characters), a background colour, and the names of its four neighbours.
//!
//! Rooms connect Hollow-Knight style: there are no portals. Walk off an edge and
//! — if a neighbour is declared on that side — the room is swapped and the player
//! reappears at the matching edge of the new room. The camera is bounded to the
//! current room ([`crate::camera`]), so each room reads as its own space.

use std::collections::HashMap;

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::prelude::*;

use crate::GameSet;
use crate::hazards::{Hazard, RespawnPoint, RockSpawner, RockSprite, SPIKE_HALF};
use crate::physics::{Solids, TILE};
use crate::player::{JumpState, Player, Velocity};
use crate::ron::{self, RonError};
use crate::state::GameState;

/// One map's data, read from a `.map.ron` file by [`MapLoader`].
#[derive(Asset, TypePath, Clone)]
pub struct MapData {
    /// A human-friendly display name (empty → falls back to the file key).
    pub name: String,
    /// Characters treated as solid tiles (e.g. `"#"`).
    pub solid: String,
    /// Characters that are deadly ground spikes (e.g. `"^"`).
    pub spikes: String,
    /// Characters that spawn falling rocks (e.g. `"R"`).
    pub rocks: String,
    /// Neighbouring map reached by walking off the top edge (empty = none).
    pub north: String,
    /// Neighbouring map reached by walking off the bottom edge (empty = none).
    pub south: String,
    /// Neighbouring map reached by walking off the right edge (empty = none).
    pub east: String,
    /// Neighbouring map reached by walking off the left edge (empty = none).
    pub west: String,
    /// Background (clear) colour as `[r, g, b]` in 0..1.
    pub bg: [f32; 3],
    /// The grid, one string per row (top to bottom).
    pub tiles: Vec<String>,
}

impl MapData {
    /// Build a map from RON text (see [`crate::ron`] for the supported subset).
    fn from_ron(text: &str) -> Result<Self, RonError> {
        let value = ron::from_str(text)?;

        let optional_str = |name: &str| -> Result<String, RonError> {
            match value.try_field(name) {
                Some(v) => Ok(v.as_str()?.to_string()),
                None => Ok(String::new()),
            }
        };

        let bg_list = value.field("bg")?.as_list()?;
        if bg_list.len() != 3 {
            return Err(RonError("bg must have 3 components".into()));
        }
        let bg = [
            bg_list[0].as_f32()?,
            bg_list[1].as_f32()?,
            bg_list[2].as_f32()?,
        ];

        let tiles = value
            .field("tiles")?
            .as_list()?
            .iter()
            .map(|v| Ok(v.as_str()?.to_string()))
            .collect::<Result<Vec<_>, RonError>>()?;

        Ok(MapData {
            name: optional_str("name")?,
            solid: value.field("solid")?.as_str()?.to_string(),
            spikes: optional_str("spikes")?,
            rocks: optional_str("rocks")?,
            north: optional_str("north")?,
            south: optional_str("south")?,
            east: optional_str("east")?,
            west: optional_str("west")?,
            bg,
            tiles,
        })
    }

    fn bg_color(&self) -> Color {
        Color::srgb(self.bg[0], self.bg[1], self.bg[2])
    }

    /// The display name, or the file `key` when none has been set.
    pub(crate) fn display_name<'a>(&'a self, key: &'a str) -> &'a str {
        if self.name.is_empty() {
            key
        } else {
            &self.name
        }
    }

    /// Serialise back to the `.map.ron` text our reader accepts (for the editor).
    #[cfg(any(test, debug_assertions))]
    pub(crate) fn to_ron(&self) -> String {
        let rows: String = self
            .tiles
            .iter()
            .map(|row| format!("        \"{row}\",\n"))
            .collect();
        format!(
            "(\n    name: \"{}\",\n    solid: \"{}\",\n    spikes: \"{}\",\n    rocks: \"{}\",\n    \
             north: \"{}\",\n    south: \"{}\",\n    east: \"{}\",\n    west: \"{}\",\n    \
             bg: [{}, {}, {}],\n    tiles: [\n{rows}    ],\n)\n",
            self.name,
            self.solid,
            self.spikes,
            self.rocks,
            self.north,
            self.south,
            self.east,
            self.west,
            self.bg[0],
            self.bg[1],
            self.bg[2],
        )
    }

    fn neighbor(&self, dir: Dir) -> Option<String> {
        let name = match dir {
            Dir::North => &self.north,
            Dir::South => &self.south,
            Dir::East => &self.east,
            Dir::West => &self.west,
        };
        (!name.is_empty()).then(|| name.clone())
    }
}

/// Loads [`MapData`] from `assets/maps/*.map.ron` using our own RON reader, so the
/// game depends only on Bevy (no `serde`/`ron`/`bevy_common_assets`).
#[derive(Default, TypePath)]
struct MapLoader;

impl AssetLoader for MapLoader {
    type Asset = MapData;
    type Settings = ();
    type Error = RonError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        _ctx: &mut LoadContext<'_>,
    ) -> Result<MapData, RonError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| RonError(format!("read error: {e}")))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| RonError(format!("not UTF-8: {e}")))?;
        MapData::from_ron(text)
    }

    fn extensions(&self) -> &[&str] {
        &["map.ron"]
    }
}

/// A cardinal direction of travel between rooms.
#[derive(Clone, Copy)]
enum Dir {
    North,
    South,
    East,
    West,
}

/// How the player should be placed when a room loads.
#[derive(Clone)]
enum Entry {
    /// The world's starting position (the `@` marker in the grid).
    Start,
    /// Arrived by walking off an edge; carries the direction travelled and the
    /// player's position along that edge (used to line up horizontal corridors).
    FromEdge(Dir, f32),
}

/// Tags every entity belonging to the current map (despawned on transition).
#[derive(Component)]
pub(crate) struct MapEntity;

#[derive(Resource)]
pub(crate) struct GameAssets {
    pub(crate) maps: HashMap<String, Handle<MapData>>,
    /// Discovered room names, sorted (drives loading and the world map).
    pub(crate) room_names: Vec<String>,
    pub(crate) tile: Handle<Image>,
    pub(crate) player: Handle<Image>,
    pub(crate) spikes: Handle<Image>,
}

/// The room the player is currently in: its name, neighbours and pixel size.
#[derive(Resource, Default)]
pub(crate) struct CurrentRoom {
    pub(crate) name: String,
    north: Option<String>,
    south: Option<String>,
    east: Option<String>,
    west: Option<String>,
    size: Vec2,
}

/// The current room's pixel bounds, read by the camera to clamp itself. `snap`
/// asks the camera to jump (rather than glide) on the next frame after a load.
#[derive(Resource, Default)]
pub(crate) struct RoomView {
    pub size: Vec2,
    pub snap: bool,
}

/// Request to (re)load a map and place the player.
#[derive(Message, Clone)]
struct LoadMap {
    map: String,
    entry: Entry,
}

/// Brief window after a transition during which edges are ignored, so the player
/// doesn't immediately bounce back through the edge they just arrived at.
#[derive(Resource, Default)]
struct TransitionCooldown(f32);

/// Where room files live on disk. The working dir is the crate root (Bevy's
/// asset root is the sibling `assets/`), so this matches the asset paths below.
pub(crate) const MAPS_DIR: &str = "assets/maps";
const START_MAP: &str = "r0_0";

/// Path the asset server loads a room from (relative to the `assets/` root).
fn map_asset_path(name: &str) -> String {
    format!("maps/{name}.map.ron")
}

/// Filesystem path to a room's file (used by the test and the editor's writes).
#[cfg(any(test, debug_assertions))]
pub(crate) fn map_fs_path(name: &str) -> String {
    format!("{MAPS_DIR}/{name}.map.ron")
}

/// Every `*.map.ron` in [`MAPS_DIR`], sorted. Dropping in a file adds a room, and
/// the level builder can save new ones here.
pub(crate) fn discover_rooms() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(MAPS_DIR)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|n| n.strip_suffix(".map.ron"))
                .map(str::to_string)
        })
        .collect();
    names.sort();
    names
}
/// Grid character marking the player's start position in the starting room.
pub(crate) const START_MARKER: char = '@';
/// Left tile column of the 2-wide vertical shaft shared by every room.
const SHAFT_COL: f32 = 9.0;
/// How far inside a room (in pixels) the player appears after a transition.
const INSET: f32 = TILE * 1.6;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<MapData>()
            .register_asset_loader(MapLoader)
            .add_message::<LoadMap>()
            .init_resource::<Solids>()
            .init_resource::<TransitionCooldown>()
            .init_resource::<CurrentRoom>()
            .init_resource::<RoomView>()
            .add_systems(Startup, load_assets)
            .add_systems(Update, wait_for_load.run_if(in_state(GameState::Loading)))
            .add_systems(OnEnter(GameState::Playing), enter_playing)
            .add_systems(
                Update,
                (
                    handle_load_map,
                    tick_cooldown,
                    detect_transitions.in_set(GameSet::Transitions),
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn load_assets(mut commands: Commands, asset_server: Res<AssetServer>) {
    let room_names = discover_rooms();
    let maps = room_names
        .iter()
        .map(|name| (name.clone(), asset_server.load(map_asset_path(name))))
        .collect();

    commands.insert_resource(GameAssets {
        maps,
        room_names,
        tile: asset_server.load("sprites/tile.png"),
        player: asset_server.load("sprites/player.png"),
        spikes: asset_server.load("sprites/spikes.png"),
    });
    commands.insert_resource(RockSprite(asset_server.load("sprites/rock.png")));
}

fn wait_for_load(
    assets: Res<GameAssets>,
    rock: Res<RockSprite>,
    asset_server: Res<AssetServer>,
    mut next: ResMut<NextState<GameState>>,
) {
    let maps_ready = assets
        .maps
        .values()
        .all(|h| asset_server.is_loaded_with_dependencies(h.id()));
    let sprites_ready = asset_server.is_loaded_with_dependencies(assets.tile.id())
        && asset_server.is_loaded_with_dependencies(assets.player.id())
        && asset_server.is_loaded_with_dependencies(assets.spikes.id())
        && asset_server.is_loaded_with_dependencies(rock.0.id());

    if maps_ready && sprites_ready {
        next.set(GameState::Playing);
    }
}

fn enter_playing(
    mut commands: Commands,
    assets: Res<GameAssets>,
    current: Res<CurrentRoom>,
    players: Query<(), With<Player>>,
    mut load: MessageWriter<LoadMap>,
) {
    // Spawn the player once; `handle_load_map` positions it for each room. (We
    // re-enter `Playing` after the level builder, so it may already exist.)
    if players.is_empty() {
        commands.spawn((
            Player,
            Velocity::default(),
            JumpState::default(),
            Sprite {
                image: assets.player.clone(),
                custom_size: Some(Vec2::new(24.0, 40.0)),
                ..default()
            },
            Transform::from_xyz(0.0, 0.0, 10.0),
        ));
    }

    // Load the current room (reloading it after editing), falling back to the
    // start room or any room if it's gone (the builder can delete/move rooms).
    let map = if assets.maps.contains_key(&current.name) {
        current.name.clone()
    } else if assets.maps.contains_key(START_MAP) {
        START_MAP.to_string()
    } else {
        assets.room_names.first().cloned().unwrap_or_default()
    };
    load.write(LoadMap {
        map,
        entry: Entry::Start,
    });
}

/// Where the player lands when a room loads, given how they entered it.
fn entry_position(entry: &Entry, room: Vec2, start: Vec2) -> Vec2 {
    match entry {
        Entry::Start => start,
        // Walked east → appear at the west corridor (and vice-versa), keeping the
        // same height so ground-level corridors line up.
        Entry::FromEdge(Dir::East, y) => Vec2::new(INSET, *y),
        Entry::FromEdge(Dir::West, y) => Vec2::new(room.x - INSET, *y),
        // Fell south → drop in at the top onto the catch ledge beside the shaft.
        Entry::FromEdge(Dir::South, _) => Vec2::new(12.5 * TILE, room.y - INSET),
        // Climbed north → land on solid floor just left of the shaft's floor gap
        // (clear of both the shaft and any left-corner spikes).
        Entry::FromEdge(Dir::North, _) => {
            Vec2::new((SHAFT_COL - 2.0) * TILE + TILE / 2.0, TILE * 2.0)
        }
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn handle_load_map(
    mut commands: Commands,
    mut events: MessageReader<LoadMap>,
    map_assets: Res<Assets<MapData>>,
    assets: Res<GameAssets>,
    mut solids: ResMut<Solids>,
    mut cooldown: ResMut<TransitionCooldown>,
    mut current: ResMut<CurrentRoom>,
    mut room_view: ResMut<RoomView>,
    mut respawn: ResMut<RespawnPoint>,
    existing: Query<Entity, With<MapEntity>>,
    mut player: Query<(&mut Transform, &mut Velocity), With<Player>>,
) {
    let Some(load) = events.read().last().cloned() else {
        return;
    };

    // Clear the old room.
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    solids.0.clear();

    let Some(handle) = assets.maps.get(&load.map) else {
        warn!("unknown map '{}'", load.map);
        return;
    };
    let Some(map) = map_assets.get(handle) else {
        warn!("map '{}' not loaded yet", load.map);
        return;
    };

    let height = map.tiles.len() as i32;
    let width = map
        .tiles
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(0) as i32;
    let mut start_pos = Vec2::new(2.0 * TILE, 2.0 * TILE);

    for (r, line) in map.tiles.iter().enumerate() {
        let row = height - 1 - r as i32; // flip so row 0 is the top, y points up
        for (c, ch) in line.chars().enumerate() {
            let col = c as i32;
            let center = Vec2::new(
                col as f32 * TILE + TILE / 2.0,
                row as f32 * TILE + TILE / 2.0,
            );

            if ch == START_MARKER {
                start_pos = center;
            } else if map.solid.contains(ch) {
                solids.0.insert((col, row));
                commands.spawn((
                    MapEntity,
                    Sprite {
                        image: assets.tile.clone(),
                        custom_size: Some(Vec2::splat(TILE)),
                        ..default()
                    },
                    Transform::from_xyz(center.x, center.y, 0.0),
                ));
            } else if map.spikes.contains(ch) {
                commands.spawn((
                    MapEntity,
                    Hazard { half: SPIKE_HALF },
                    Sprite {
                        image: assets.spikes.clone(),
                        custom_size: Some(Vec2::splat(TILE)),
                        ..default()
                    },
                    Transform::from_xyz(center.x, center.y, 1.0),
                ));
            } else if map.rocks.contains(ch) {
                // Stagger spawners so they don't all drop in sync.
                let mut timer = Timer::from_seconds(2.2, TimerMode::Repeating);
                timer.set_elapsed(std::time::Duration::from_secs_f32(
                    (col as f32 * 0.41) % 2.2,
                ));
                commands.spawn((
                    MapEntity,
                    RockSpawner { timer },
                    Transform::from_xyz(center.x, center.y, 1.0),
                ));
            }
        }
    }

    let room = Vec2::new(width as f32 * TILE, height as f32 * TILE);
    let pos = entry_position(&load.entry, room, start_pos);

    // Record the room's name, neighbours + size for edge detection and the camera.
    *current = CurrentRoom {
        name: load.map.clone(),
        north: map.neighbor(Dir::North),
        south: map.neighbor(Dir::South),
        east: map.neighbor(Dir::East),
        west: map.neighbor(Dir::West),
        size: room,
    };
    room_view.size = room;
    room_view.snap = true;
    respawn.0 = pos; // dying returns the player to where they entered
    commands.insert_resource(ClearColor(map.bg_color()));

    if let Ok((mut transform, mut velocity)) = player.single_mut() {
        transform.translation.x = pos.x;
        transform.translation.y = pos.y;
        velocity.0 = Vec2::ZERO;
    }
    cooldown.0 = 0.4;
}

fn tick_cooldown(time: Res<Time>, mut cooldown: ResMut<TransitionCooldown>) {
    cooldown.0 = (cooldown.0 - time.delta_secs()).max(0.0);
}

/// Swap rooms when the player walks off an edge that has a neighbour; if they fall
/// off a bottomless edge (no south neighbour), respawn them where they came in.
fn detect_transitions(
    cooldown: Res<TransitionCooldown>,
    current: Res<CurrentRoom>,
    respawn: Res<RespawnPoint>,
    mut player: Query<(&mut Transform, &mut Velocity), With<Player>>,
    mut load: MessageWriter<LoadMap>,
) {
    let Ok((mut transform, mut velocity)) = player.single_mut() else {
        return;
    };
    let pos = transform.translation.truncate();
    let room = current.size;

    if cooldown.0 <= 0.0 {
        let target = if pos.x < 0.0 {
            current.west.clone().map(|m| (m, Dir::West, pos.y))
        } else if pos.x > room.x {
            current.east.clone().map(|m| (m, Dir::East, pos.y))
        } else if pos.y > room.y {
            current.north.clone().map(|m| (m, Dir::North, pos.x))
        } else if pos.y < 0.0 {
            current.south.clone().map(|m| (m, Dir::South, pos.x))
        } else {
            None
        };

        if let Some((map, dir, coord)) = target {
            load.write(LoadMap {
                map,
                entry: Entry::FromEdge(dir, coord),
            });
            return;
        }
    }

    // Fell into a bottomless pit (no room below): respawn at the room entrance.
    if pos.y < -TILE && current.south.is_none() {
        transform.translation.x = respawn.0.x;
        transform.translation.y = respawn.0.y;
        velocity.0 = Vec2::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every room on disk must parse with our RON reader, have content, and only
    /// name neighbours that actually exist.
    #[test]
    fn demo_maps_parse_and_interconnect() {
        let names = discover_rooms();
        assert!(
            names.len() >= 12,
            "expected at least the 12 demo rooms, found {}",
            names.len()
        );

        let mut maps = HashMap::new();
        for name in &names {
            let path = map_fs_path(name);
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"));
            let map = MapData::from_ron(&text).unwrap_or_else(|e| panic!("parsing {path}: {e}"));

            assert!(!map.solid.is_empty(), "{name}: empty solid legend");
            assert!(!map.tiles.is_empty(), "{name}: no rows");
            maps.insert(name.clone(), map);
        }

        // Every declared neighbour must reference a real room. (Links need not be
        // symmetric — one-way passages are allowed, and the builder can author
        // rooms — but a dangling link is a bug.)
        for (name, map) in &maps {
            for dir in [Dir::North, Dir::South, Dir::East, Dir::West] {
                if let Some(target) = map.neighbor(dir) {
                    assert!(
                        maps.contains_key(&target),
                        "{name}: links to unknown room '{target}'"
                    );
                }
            }
        }

        // The starting room must contain the start marker.
        let start = &maps[START_MAP];
        assert!(
            start.tiles.iter().any(|r| r.contains(START_MARKER)),
            "{START_MAP}: missing start marker '{START_MARKER}'"
        );
    }

    /// What the level builder writes must round-trip back through our reader.
    #[test]
    fn to_ron_round_trips() {
        let original = MapData {
            name: "Forest Glade".to_string(),
            solid: "#".to_string(),
            spikes: "^".to_string(),
            rocks: "R".to_string(),
            north: "r0_1".to_string(),
            south: String::new(),
            east: "r1_0".to_string(),
            west: String::new(),
            bg: [0.25, 0.5, 0.75],
            tiles: vec![
                "####".to_string(),
                "#.@#".to_string(),
                "#^R#".to_string(),
                "####".to_string(),
            ],
        };

        let parsed = MapData::from_ron(&original.to_ron()).expect("round-trip parse");
        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.solid, original.solid);
        assert_eq!(parsed.north, original.north);
        assert_eq!(parsed.south, original.south);
        assert_eq!(parsed.east, original.east);
        assert_eq!(parsed.bg, original.bg);
        assert_eq!(parsed.tiles, original.tiles);
    }
}
