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
use crate::hazards::{Hazard, RespawnPoint, Rock, RockSpawner, RockSprite, SPIKE_HALF};
use crate::health::{Health, Hurt};
use crate::input::PlayerIntent;
use crate::physics::{Solids, TILE};
use crate::player::{JumpState, PLAYER_HALF, Player, Velocity};
use crate::ron::{self, RonError};
use crate::save::{self, Save};
use crate::state::GameState;

/// A teleporter pad, stored as pure coordinates: stepping on the cell
/// `(origin_col, origin_row)` sends the player to room `to` at `(dest_col,
/// dest_row)`. All are grid cells (row 0 = top, as written in `tiles`). Because a
/// pad needs no grid glyph, it never competes with tile characters — a room may
/// hold many pads, and a pad may even target another cell in its own room.
#[derive(Clone)]
pub struct Teleport {
    pub origin_col: i32,
    pub origin_row: i32,
    pub to: String,
    pub dest_col: i32,
    pub dest_row: i32,
}

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
    /// Teleporter pads in this room (empty = none).
    pub teleports: Vec<Teleport>,
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

        let teleports = match value.try_field("teleports") {
            Some(list) => list
                .as_list()?
                .iter()
                .map(|t| {
                    Ok(Teleport {
                        origin_col: t.field("origin_col")?.as_i32()?,
                        origin_row: t.field("origin_row")?.as_i32()?,
                        to: t.field("to")?.as_str()?.to_string(),
                        dest_col: t.field("dest_col")?.as_i32()?,
                        dest_row: t.field("dest_row")?.as_i32()?,
                    })
                })
                .collect::<Result<Vec<_>, RonError>>()?,
            None => Vec::new(),
        };

        Ok(MapData {
            name: optional_str("name")?,
            solid: value.field("solid")?.as_str()?.to_string(),
            spikes: optional_str("spikes")?,
            rocks: optional_str("rocks")?,
            north: optional_str("north")?,
            south: optional_str("south")?,
            east: optional_str("east")?,
            west: optional_str("west")?,
            teleports,
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
        let teleports: String = self
            .teleports
            .iter()
            .map(|t| {
                format!(
                    "        (origin_col: {}, origin_row: {}, to: \"{}\", dest_col: {}, dest_row: {}),\n",
                    t.origin_col, t.origin_row, t.to, t.dest_col, t.dest_row
                )
            })
            .collect();
        format!(
            "(\n    name: \"{}\",\n    solid: \"{}\",\n    spikes: \"{}\",\n    rocks: \"{}\",\n    \
             north: \"{}\",\n    south: \"{}\",\n    east: \"{}\",\n    west: \"{}\",\n    \
             teleports: [\n{teleports}    ],\n    bg: [{}, {}, {}],\n    tiles: [\n{rows}    ],\n)\n",
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
pub(crate) enum Dir {
    North,
    South,
    East,
    West,
}

/// How the player should be placed when a room loads.
#[derive(Clone)]
pub(crate) enum Entry {
    /// The world's starting position (the `@` marker in the grid).
    Start,
    /// Arrived by walking off an edge; carries the direction travelled and the
    /// player's position along that edge (used to line up horizontal corridors).
    FromEdge(Dir, f32),
    /// Arrived through a teleporter; place the player at the destination cell
    /// `(col, row)` (grid coords, row 0 = top) recorded on the pad.
    Teleport(i32, i32),
    /// Respawned/loaded onto a bench at cell `(col, row)` (grid coords).
    Bench(i32, i32),
}

/// Tags every entity belonging to the current map (despawned on transition).
#[derive(Component)]
pub(crate) struct MapEntity;

/// A spawned teleporter pad: stepping onto it sends the player to room `to`, the
/// cell `dest` `(col, row)` in grid coords (row 0 = top).
#[derive(Component)]
pub(crate) struct Teleporter {
    to: String,
    dest: (i32, i32),
}

/// Whether teleporters are ready to fire. Disarmed on each teleport, on every room
/// load, and when the player respawns from damage (so respawning onto a pad can't
/// chain), and only re-armed once the player is [`TELEPORT_REARM`] from every pad.
#[derive(Resource)]
pub(crate) struct TeleportArmed(pub(crate) bool);

impl Default for TeleportArmed {
    fn default() -> Self {
        Self(true)
    }
}

/// Grid glyph marking a bench: a checkpoint that saves the game, refills hearts,
/// resets enemies, and becomes the player's respawn point.
pub(crate) const BENCH_GLYPH: char = 'B';
/// Half-extents of a bench's "rest" trigger area.
const BENCH_HALF: Vec2 = Vec2::new(TILE * 0.5, TILE * 0.5);

/// A spawned bench, carrying its own grid cell so resting can record it as the
/// respawn point.
#[derive(Component)]
pub(crate) struct Bench {
    col: i32,
    row: i32,
}

/// The "press to rest" prompt shown above a bench the player is standing on.
#[derive(Component)]
struct BenchPrompt;

/// A menu-driven request for where to spawn when `Playing` next begins: the room,
/// and an optional bench cell (else the room's start marker). Consumed by
/// [`enter_playing`]; when unset, re-entering `Playing` (from the editor) just
/// reloads the current room.
#[derive(Resource, Default)]
pub(crate) struct PendingSpawn(pub(crate) Option<SpawnRequest>);

pub(crate) struct SpawnRequest {
    pub(crate) room: String,
    pub(crate) at_cell: Option<(i32, i32)>,
}
/// Half-extents of a teleporter's trigger area.
const TELEPORT_HALF: Vec2 = Vec2::new(TILE * 0.45, TILE * 0.5);
/// How far the player must move from a pad before it re-arms. Larger than the
/// trigger, so there's a dead zone (~1.5 tiles) between using a pad and it being
/// able to fire again — you must step clear before re-entering.
const TELEPORT_REARM: f32 = TILE * 1.5;

#[derive(Resource)]
pub(crate) struct GameAssets {
    pub(crate) maps: HashMap<String, Handle<MapData>>,
    /// Discovered room names, sorted (drives loading and the world map).
    pub(crate) room_names: Vec<String>,
    pub(crate) tile: Handle<Image>,
    pub(crate) player: Handle<Image>,
    pub(crate) spikes: Handle<Image>,
    pub(crate) portal: Handle<Image>,
    pub(crate) bench: Handle<Image>,
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
pub(crate) struct LoadMap {
    pub(crate) map: String,
    pub(crate) entry: Entry,
}

/// Brief window after a transition during which edges are ignored, so the player
/// doesn't immediately bounce back through the edge they just arrived at.
#[derive(Resource, Default)]
struct TransitionCooldown(f32);

/// Where room files live on disk. The working dir is the crate root (Bevy's
/// asset root is the sibling `assets/`), so this matches the asset paths below.
pub(crate) const MAPS_DIR: &str = "assets/maps";
pub(crate) const START_MAP: &str = "r0_0";

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
            .init_resource::<TeleportArmed>()
            .init_resource::<PendingSpawn>()
            .add_systems(Startup, load_assets)
            .add_systems(Update, wait_for_load.run_if(in_state(GameState::Loading)))
            .add_systems(
                OnEnter(GameState::Playing),
                (enter_playing, spawn_bench_prompt),
            )
            .add_systems(OnExit(GameState::Playing), despawn_bench_prompt)
            .add_systems(
                Update,
                (
                    handle_load_map,
                    tick_cooldown,
                    detect_transitions.in_set(GameSet::Transitions),
                    detect_teleport.in_set(GameSet::Transitions),
                    bench_interact.in_set(GameSet::Transitions),
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
        portal: asset_server.load("sprites/portal.png"),
        bench: asset_server.load("sprites/bench.png"),
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
        && asset_server.is_loaded_with_dependencies(assets.portal.id())
        && asset_server.is_loaded_with_dependencies(assets.bench.id())
        && asset_server.is_loaded_with_dependencies(rock.0.id());

    if maps_ready && sprites_ready {
        next.set(GameState::Playing);
    }
}

fn enter_playing(
    mut commands: Commands,
    assets: Res<GameAssets>,
    current: Res<CurrentRoom>,
    mut pending: ResMut<PendingSpawn>,
    mut health: ResMut<Health>,
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

    // A menu-driven New Game / Load Game requests a room (and maybe a bench cell);
    // honour it and refill hearts. Otherwise we're returning from the editor — just
    // reload the current room, falling back to the start (the builder can move rooms).
    if let Some(req) = pending.0.take() {
        health.current = health.max;
        let entry = match req.at_cell {
            Some((col, row)) => Entry::Bench(col, row),
            None => Entry::Start,
        };
        let map = if assets.maps.contains_key(&req.room) {
            req.room
        } else {
            START_MAP.to_string()
        };
        load.write(LoadMap { map, entry });
        return;
    }

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

/// Where the player lands when a room loads, given how they entered it. `teleport`
/// is the resolved destination-cell centre (falls back to `start` if out of range).
fn entry_position(entry: &Entry, room: Vec2, start: Vec2, teleport: Vec2) -> Vec2 {
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
        // Teleported / respawned at a bench → land on the recorded cell.
        Entry::Teleport(..) | Entry::Bench(..) => teleport,
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
    mut armed: ResMut<TeleportArmed>,
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
            } else if ch == BENCH_GLYPH {
                commands.spawn((
                    MapEntity,
                    Bench { col, row: r as i32 },
                    Sprite {
                        image: assets.bench.clone(),
                        custom_size: Some(Vec2::splat(TILE)),
                        ..default()
                    },
                    Transform::from_xyz(center.x, center.y, 1.0),
                ));
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

    // Teleporter pads are pure data (no grid glyph) — spawn one per portal at its
    // origin cell, flipping the row so y points up.
    for tp in &map.teleports {
        if (0..width).contains(&tp.origin_col) && (0..height).contains(&tp.origin_row) {
            let world_row = height - 1 - tp.origin_row;
            commands.spawn((
                MapEntity,
                Teleporter {
                    to: tp.to.clone(),
                    dest: (tp.dest_col, tp.dest_row),
                },
                Sprite {
                    image: assets.portal.clone(),
                    custom_size: Some(Vec2::splat(TILE)),
                    ..default()
                },
                Transform::from_xyz(
                    tp.origin_col as f32 * TILE + TILE / 2.0,
                    world_row as f32 * TILE + TILE / 2.0,
                    1.0,
                ),
            ));
        }
    }

    // Resolve a destination cell (teleporter or bench; grid coords, row 0 = top) to
    // a world centre, flipping the row so y points up. Out-of-range falls to `start`.
    let teleport_pos = match load.entry {
        Entry::Teleport(col, row) | Entry::Bench(col, row)
            if (0..width).contains(&col) && (0..height).contains(&row) =>
        {
            let world_row = height - 1 - row;
            Vec2::new(
                col as f32 * TILE + TILE / 2.0,
                world_row as f32 * TILE + TILE / 2.0,
            )
        }
        _ => start_pos,
    };

    let room = Vec2::new(width as f32 * TILE, height as f32 * TILE);
    let pos = entry_position(&load.entry, room, start_pos, teleport_pos);

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
    // Disarm teleporters until the player steps clear, so they don't immediately
    // re-fire on the pad they just landed on (or one under the entry point).
    armed.0 = false;
}

fn tick_cooldown(time: Res<Time>, mut cooldown: ResMut<TransitionCooldown>) {
    cooldown.0 = (cooldown.0 - time.delta_secs()).max(0.0);
}

/// Swap rooms when the player walks off an edge that has a neighbour; if they fall
/// off a bottomless edge (no south neighbour), respawn them where they came in.
fn detect_transitions(
    cooldown: Res<TransitionCooldown>,
    current: Res<CurrentRoom>,
    player: Query<&Transform, With<Player>>,
    mut load: MessageWriter<LoadMap>,
    mut hurt: MessageWriter<Hurt>,
) {
    let Ok(transform) = player.single() else {
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

    // Fell into a bottomless pit (no room below): take a hit. The damage system
    // respawns at the room entrance, or the last bench if it was the final heart.
    if pos.y < -TILE && current.south.is_none() {
        hurt.write(Hurt);
    }
}

/// Send the player to a linked room when they step onto a teleporter pad. A pad
/// fires only when [`TeleportArmed`], which re-arms only once the player is
/// [`TELEPORT_REARM`] clear of every pad — the dead zone between that and the
/// (smaller) trigger stops a single pad chaining teleports or firing on arrival.
fn detect_teleport(
    mut armed: ResMut<TeleportArmed>,
    teleporters: Query<(&Transform, &Teleporter)>,
    player: Query<&Transform, With<Player>>,
    mut load: MessageWriter<LoadMap>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // Re-arm only when well clear of every pad (not merely off the trigger box).
    let clear = teleporters
        .iter()
        .all(|(tf, _)| player_pos.distance(tf.translation.truncate()) > TELEPORT_REARM);
    if clear {
        armed.0 = true;
    }
    if !armed.0 {
        return;
    }

    // Armed and overlapping a pad → teleport, then disarm until clear again.
    if let Some((_, tp)) = teleporters.iter().find(|(tf, _)| {
        let delta = (tf.translation.truncate() - player_pos).abs();
        delta.x < TELEPORT_HALF.x + PLAYER_HALF.x && delta.y < TELEPORT_HALF.y + PLAYER_HALF.y
    }) {
        armed.0 = false;
        load.write(LoadMap {
            map: tp.to.clone(),
            entry: Entry::Teleport(tp.dest.0, tp.dest.1),
        });
    }
}

/// Show a prompt above a bench the player is standing on, and — when they press
/// **interact** there — rest: refill hearts, clear in-flight enemies, and record
/// the bench as the save + respawn point (persisted to disk).
#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn bench_interact(
    mut commands: Commands,
    intent: Res<PlayerIntent>,
    mut save: ResMut<Save>,
    mut health: ResMut<Health>,
    current: Res<CurrentRoom>,
    benches: Query<(&Transform, &Bench), Without<BenchPrompt>>,
    rocks: Query<Entity, With<Rock>>,
    player: Query<&Transform, (With<Player>, Without<BenchPrompt>)>,
    mut prompt: Query<(&mut Transform, &mut Visibility), With<BenchPrompt>>,
) {
    let Ok(player_tf) = player.single() else {
        return;
    };
    let player_pos = player_tf.translation.truncate();

    // The bench the player is standing on, if any.
    let on_bench = benches.iter().find(|(tf, _)| {
        let delta = (tf.translation.truncate() - player_pos).abs();
        delta.x < BENCH_HALF.x + PLAYER_HALF.x && delta.y < BENCH_HALF.y + PLAYER_HALF.y
    });

    // Show the "rest" prompt above that bench, or hide it.
    if let Ok((mut prompt_tf, mut visibility)) = prompt.single_mut() {
        match on_bench {
            Some((tf, _)) => {
                prompt_tf.translation.x = tf.translation.x;
                prompt_tf.translation.y = tf.translation.y + TILE;
                *visibility = Visibility::Visible;
            }
            None => *visibility = Visibility::Hidden,
        }
    }

    // Rest only on the interact press, while standing on a bench.
    if intent.interact
        && let Some((_, bench)) = on_bench
    {
        health.current = health.max;
        save.room = current.name.clone();
        save.bench_room = current.name.clone();
        save.bench_col = bench.col;
        save.bench_row = bench.row;
        save::write_save(&save);
        for entity in &rocks {
            commands.entity(entity).despawn();
        }
    }
}

/// Spawn the (initially hidden) bench prompt; [`bench_interact`] positions it.
fn spawn_bench_prompt(mut commands: Commands, existing: Query<(), With<BenchPrompt>>) {
    if !existing.is_empty() {
        return;
    }
    commands.spawn((
        BenchPrompt,
        Text2d::new("[E] rest"),
        TextFont {
            font_size: FontSize::Px(16.0),
            ..default()
        },
        TextColor(Color::srgb(0.96, 0.9, 0.6)),
        Transform::from_xyz(0.0, 0.0, 20.0),
        Visibility::Hidden,
    ));
}

fn despawn_bench_prompt(mut commands: Commands, prompts: Query<Entity, With<BenchPrompt>>) {
    for entity in &prompts {
        commands.entity(entity).despawn();
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
            // Teleporters: origin must be in this room and dest in a real room.
            let dims = |m: &MapData| {
                (
                    m.tiles.iter().map(|r| r.chars().count()).max().unwrap_or(0) as i32,
                    m.tiles.len() as i32,
                )
            };
            let (ow, oh) = dims(map);
            for tp in &map.teleports {
                assert!(
                    (0..ow).contains(&tp.origin_col) && (0..oh).contains(&tp.origin_row),
                    "{name}: teleporter origin ({}, {}) out of bounds",
                    tp.origin_col,
                    tp.origin_row
                );
                let dest = maps
                    .get(&tp.to)
                    .unwrap_or_else(|| panic!("{name}: teleporter to unknown room '{}'", tp.to));
                let (dw, dh) = dims(dest);
                assert!(
                    (0..dw).contains(&tp.dest_col) && (0..dh).contains(&tp.dest_row),
                    "{name}: teleporter dest ({}, {}) out of bounds in '{}'",
                    tp.dest_col,
                    tp.dest_row,
                    tp.to
                );
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
            teleports: vec![Teleport {
                origin_col: 1,
                origin_row: 1,
                to: "r1_1".to_string(),
                dest_col: 7,
                dest_row: 2,
            }],
            bg: [0.25, 0.5, 0.75],
            tiles: vec![
                "####".to_string(),
                "#T@#".to_string(),
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
        assert_eq!(parsed.teleports.len(), 1);
        assert_eq!(parsed.teleports[0].origin_col, 1);
        assert_eq!(parsed.teleports[0].origin_row, 1);
        assert_eq!(parsed.teleports[0].to, "r1_1");
        assert_eq!(parsed.teleports[0].dest_col, 7);
        assert_eq!(parsed.teleports[0].dest_row, 2);
    }
}
