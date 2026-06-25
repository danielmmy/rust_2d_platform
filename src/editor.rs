//! A dev-only level builder (compiled only in debug builds — see `main.rs`).
//!
//! Press `F2` while playing to open it. The builder has two views:
//!
//! - **Tiles** — paint the current room with the game's own sprites, resize it
//!   freely, recolour it, and save.
//! - **Rooms** (`M`) — a map of every room where you select one to edit, **add**,
//!   **delete**, **reorder** (grab + move), or **reset** to the default 12.
//!
//! The room grid is **unbounded** (the room view scrolls). Connectivity is derived
//! from the grid: rooms named `r{col}_{row}` are linked to their existing
//! orthogonal neighbours automatically, so there are no link controls to fiddle
//! with. Structural changes rewrite the affected `.map.ron` files and update the
//! running game; `Esc` from Tiles leaves the builder.

use std::collections::{BTreeMap, HashSet};

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::hazards::RockSprite;
use crate::menu::Paused;
use crate::state::GameState;
use crate::world::{CurrentRoom, GameAssets, MapData, START_MARKER, Teleport, map_fs_path};
use crate::worldmap::MapView;

/// The paint brushes, by the grid character they write.
const BRUSHES: [(char, &str); 5] = [
    ('#', "Wall"),
    ('^', "Spike"),
    ('R', "Rock"),
    (START_MARKER, "Start"),
    ('.', "Erase"),
];

/// Dark room tints to cycle through with `B`.
const PALETTE: [[f32; 3]; 8] = [
    [0.26, 0.12, 0.12],
    [0.26, 0.18, 0.10],
    [0.20, 0.24, 0.10],
    [0.10, 0.24, 0.15],
    [0.10, 0.22, 0.25],
    [0.11, 0.14, 0.27],
    [0.19, 0.10, 0.27],
    [0.27, 0.10, 0.20],
];

// The room-map view is an unbounded grid shown through a fixed window of cells.
const VIEW_COLS: i32 = 7;
const VIEW_ROWS: i32 = 5;
const GRID_W: f32 = 840.0;
const GRID_H: f32 = 372.0;

/// Which builder view is showing.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq)]
enum EditorView {
    #[default]
    Tiles,
    Rooms,
}

/// The room currently being painted (a working copy, applied on save).
#[derive(Resource, Default)]
struct EditBuffer {
    name: String,         // the file key (e.g. "r0_0")
    display: String,      // the human-friendly name (empty → shows the key)
    grid: Vec<Vec<char>>, // [row][col]; row 0 is the top
    north: String,
    south: String,
    east: String,
    west: String,
    teleports: Vec<Teleport>, // preserved across edits (no painting UI yet)
    bg: [f32; 3],
    bg_index: usize,
    cursor: (usize, usize), // (col, row)
    brush: usize,
    rename: Option<String>, // Some(text) while typing a new display name
    status: String,
}

/// The cursor in the room-map view.
#[derive(Resource, Default)]
struct RoomMap {
    gx: i32,
    gy: i32,
    grab: Option<(i32, i32)>, // a picked-up room awaiting a drop
    confirm_reset: bool,
    status: String,
}

/// Set by the menu's "Level Builder" entry; consumed once we're in `Playing`.
#[derive(Resource, Default)]
pub(crate) struct StartInEditor(pub bool);

/// Tags every entity that makes up the builder overlay (despawned on redraw).
#[derive(Component)]
struct EditorEntity;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditBuffer>()
            .init_resource::<RoomMap>()
            .init_resource::<EditorView>()
            .init_resource::<StartInEditor>()
            .add_systems(
                Update,
                (enter_editor, launch_from_menu).run_if(
                    in_state(GameState::Playing)
                        .and_then(in_state(MapView::Closed))
                        .and_then(in_state(Paused::Running)),
                ),
            )
            .add_systems(OnEnter(GameState::Editor), open_editor)
            .add_systems(OnExit(GameState::Editor), close_editor)
            .add_systems(
                Update,
                edit_tiles.run_if(in_state(GameState::Editor).and_then(in_tiles)),
            )
            .add_systems(
                Update,
                edit_rooms.run_if(in_state(GameState::Editor).and_then(in_rooms)),
            );
    }
}

fn in_tiles(view: Res<EditorView>) -> bool {
    matches!(*view, EditorView::Tiles)
}

fn in_rooms(view: Res<EditorView>) -> bool {
    matches!(*view, EditorView::Rooms)
}

fn enter_editor(keys: Res<ButtonInput<KeyCode>>, mut next: ResMut<NextState<GameState>>) {
    if keys.just_pressed(KeyCode::F2) {
        next.set(GameState::Editor);
    }
}

fn launch_from_menu(mut flag: ResMut<StartInEditor>, mut next: ResMut<NextState<GameState>>) {
    if flag.0 {
        flag.0 = false;
        next.set(GameState::Editor);
    }
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn open_editor(
    mut buffer: ResMut<EditBuffer>,
    mut view: ResMut<EditorView>,
    mut room: ResMut<RoomMap>,
    mut commands: Commands,
    current: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    maps: Res<Assets<MapData>>,
    rock: Res<RockSprite>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    let name = if current.name.is_empty() {
        "r0_0".to_string()
    } else {
        current.name.clone()
    };
    *view = EditorView::Tiles;
    *room = RoomMap::default();
    if let Some((gx, gy)) = parse_pos(&name) {
        room.gx = gx;
        room.gy = gy;
    }
    *buffer = load_buffer(&name, &assets, &maps);
    draw_tiles(
        &mut commands,
        &buffer,
        &assets,
        &rock,
        camera_center(&camera),
    );
}

fn close_editor(mut commands: Commands, overlay: Query<Entity, With<EditorEntity>>) {
    for entity in &overlay {
        commands.entity(entity).despawn();
    }
}

// --- tile view -----------------------------------------------------------

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn edit_tiles(
    keys: Res<ButtonInput<KeyCode>>,
    mut typed: MessageReader<KeyboardInput>,
    mut commands: Commands,
    mut buffer: ResMut<EditBuffer>,
    mut view: ResMut<EditorView>,
    mut game_assets: ResMut<GameAssets>,
    mut map_assets: ResMut<Assets<MapData>>,
    mut current: ResMut<CurrentRoom>,
    mut next: ResMut<NextState<GameState>>,
    room: Res<RoomMap>,
    rock: Res<RockSprite>,
    camera: Query<&Transform, With<Camera2d>>,
    overlay: Query<Entity, With<EditorEntity>>,
) {
    let center = camera_center(&camera);
    // Always drain typed keys (so none are stale when rename mode begins).
    let events: Vec<KeyboardInput> = typed.read().cloned().collect();

    // Rename mode captures all keyboard input.
    if let Some(mut text) = buffer.rename.clone() {
        match apply_typing(&mut text, &events) {
            Typing::Confirm => {
                buffer.display = text.trim().to_string();
                buffer.rename = None;
                buffer.status = save_tiles(&buffer, &mut game_assets, &mut map_assets);
            }
            Typing::Cancel => buffer.rename = None,
            Typing::Continue => buffer.rename = Some(text),
        }
        redraw(&mut commands, &overlay);
        draw_tiles(&mut commands, &buffer, &game_assets, &rock, center);
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        current.name = buffer.name.clone(); // spawn back into the room we edited
        next.set(GameState::Playing);
        return;
    }
    if keys.just_pressed(KeyCode::Enter) {
        buffer.rename = Some(buffer.display.clone());
        redraw(&mut commands, &overlay);
        draw_tiles(&mut commands, &buffer, &game_assets, &rock, center);
        return;
    }
    if keys.just_pressed(KeyCode::KeyM) {
        *view = EditorView::Rooms;
        redraw(&mut commands, &overlay);
        draw_room_map(&mut commands, center, &game_assets, &map_assets, &room);
        return;
    }

    let cols = buffer.grid.first().map_or(0, Vec::len);
    let rows = buffer.grid.len();
    let mut changed = false;

    if keys.just_pressed(KeyCode::ArrowLeft) && buffer.cursor.0 > 0 {
        buffer.cursor.0 -= 1;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowRight) && buffer.cursor.0 + 1 < cols {
        buffer.cursor.0 += 1;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowUp) && buffer.cursor.1 > 0 {
        buffer.cursor.1 -= 1;
        changed = true;
    }
    if keys.just_pressed(KeyCode::ArrowDown) && buffer.cursor.1 + 1 < rows {
        buffer.cursor.1 += 1;
        changed = true;
    }

    if keys.just_pressed(KeyCode::Space) {
        let (c, r) = buffer.cursor;
        buffer.grid[r][c] = BRUSHES[buffer.brush].0;
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        let (c, r) = buffer.cursor;
        buffer.grid[r][c] = '.';
        changed = true;
    }
    if keys.just_pressed(KeyCode::Tab) {
        buffer.brush = (buffer.brush + 1) % BRUSHES.len();
        changed = true;
    }

    if keys.just_pressed(KeyCode::BracketRight) {
        for row in &mut buffer.grid {
            row.push('.');
        }
        changed = true;
    }
    if keys.just_pressed(KeyCode::BracketLeft) && cols > 3 {
        for row in &mut buffer.grid {
            row.pop();
        }
        buffer.cursor.0 = buffer.cursor.0.min(cols - 2);
        changed = true;
    }
    if keys.just_pressed(KeyCode::Equal) {
        buffer.grid.push(vec!['.'; cols.max(1)]);
        changed = true;
    }
    if keys.just_pressed(KeyCode::Minus) && rows > 3 {
        buffer.grid.pop();
        buffer.cursor.1 = buffer.cursor.1.min(rows - 2);
        changed = true;
    }

    if keys.just_pressed(KeyCode::KeyB) {
        buffer.bg_index = (buffer.bg_index + 1) % PALETTE.len();
        buffer.bg = PALETTE[buffer.bg_index];
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyS) {
        buffer.status = save_tiles(&buffer, &mut game_assets, &mut map_assets);
        changed = true;
    }

    if changed {
        redraw(&mut commands, &overlay);
        draw_tiles(&mut commands, &buffer, &game_assets, &rock, center);
    }
}

fn draw_tiles(
    commands: &mut Commands,
    buffer: &EditBuffer,
    assets: &GameAssets,
    rock: &RockSprite,
    center: Vec2,
) {
    backdrop(commands, center);

    let rows = buffer.grid.len();
    let cols = buffer.grid.first().map_or(0, Vec::len);
    if rows == 0 || cols == 0 {
        return;
    }
    let tile = (840.0 / cols as f32).min(420.0 / rows as f32).min(40.0);
    let (gw, gh) = (cols as f32 * tile, rows as f32 * tile);
    let top_left = Vec2::new(center.x - gw / 2.0, center.y + gh / 2.0);

    box_at(
        commands,
        center,
        Vec2::new(gw, gh),
        151.0,
        lighten(buffer.bg),
    );
    for (r, row) in buffer.grid.iter().enumerate() {
        for (c, &ch) in row.iter().enumerate() {
            if let Some(image) = sprite_for(ch, assets, rock) {
                let pos = Vec2::new(
                    top_left.x + (c as f32 + 0.5) * tile,
                    top_left.y - (r as f32 + 0.5) * tile,
                );
                commands.spawn((
                    EditorEntity,
                    Sprite {
                        image,
                        custom_size: Some(Vec2::splat(tile * 0.96)),
                        ..default()
                    },
                    Transform::from_xyz(pos.x, pos.y, 152.0),
                ));
            }
        }
    }

    let (cc, cr) = buffer.cursor;
    box_at(
        commands,
        Vec2::new(
            top_left.x + (cc as f32 + 0.5) * tile,
            top_left.y - (cr as f32 + 0.5) * tile,
        ),
        Vec2::splat(tile),
        154.0,
        Color::srgba(1.0, 1.0, 1.0, 0.35),
    );

    let bright = Color::srgb(0.92, 0.94, 0.98);
    let dim = Color::srgb(0.55, 0.58, 0.66);
    let named = if buffer.display.is_empty() {
        buffer.name.clone()
    } else {
        format!("{} ({})", buffer.display, buffer.name)
    };
    text_at(
        commands,
        center + Vec2::new(0.0, 250.0),
        22.0,
        bright,
        &format!("LEVEL BUILDER — {named}   {cols}×{rows}"),
    );
    text_at(
        commands,
        center + Vec2::new(0.0, 224.0),
        14.0,
        dim,
        &buffer.status,
    );
    text_at(
        commands,
        center + Vec2::new(0.0, -218.0),
        18.0,
        bright,
        &format!("brush: {}", BRUSHES[buffer.brush].1),
    );
    text_at(
        commands,
        center + Vec2::new(0.0, -241.0),
        13.0,
        dim,
        "arrows move   ·   space paint   ·   X erase   ·   tab brush   ·   enter rename",
    );
    text_at(
        commands,
        center + Vec2::new(0.0, -259.0),
        13.0,
        dim,
        "[ ] − =  resize   ·   B colour   ·   S save   ·   M rooms   ·   esc exit",
    );

    // Rename prompt overlays the centre while typing a display name.
    if let Some(text) = &buffer.rename {
        box_at(
            commands,
            center,
            Vec2::new(580.0, 96.0),
            155.0,
            Color::srgba(0.04, 0.04, 0.09, 0.97),
        );
        text_at(
            commands,
            center + Vec2::new(0.0, 12.0),
            24.0,
            bright,
            &format!("Name: {text}_"),
        );
        text_at(
            commands,
            center + Vec2::new(0.0, -22.0),
            14.0,
            dim,
            "[enter] ok    [esc] cancel",
        );
    }
}

// --- room-map view -------------------------------------------------------

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn edit_rooms(
    keys: Res<ButtonInput<KeyCode>>,
    mut commands: Commands,
    mut view: ResMut<EditorView>,
    mut room: ResMut<RoomMap>,
    mut buffer: ResMut<EditBuffer>,
    mut game_assets: ResMut<GameAssets>,
    mut map_assets: ResMut<Assets<MapData>>,
    rock: Res<RockSprite>,
    camera: Query<&Transform, With<Camera2d>>,
    overlay: Query<Entity, With<EditorEntity>>,
) {
    let center = camera_center(&camera);

    // Leave the room map.
    if keys.just_pressed(KeyCode::KeyM)
        || (keys.just_pressed(KeyCode::Escape) && room.grab.is_none())
    {
        *view = EditorView::Tiles;
        let name = buffer.name.clone();
        *buffer = load_buffer(&name, &game_assets, &map_assets);
        redraw(&mut commands, &overlay);
        draw_tiles(&mut commands, &buffer, &game_assets, &rock, center);
        return;
    }

    let here = (room.gx, room.gy);
    let occupied = room_data(&game_assets, &map_assets, &name_at(here)).is_some();
    let mut changed = false;
    let mut status = None;

    // Cancel a grab.
    if keys.just_pressed(KeyCode::Escape) {
        room.grab = None;
        changed = true;
    }

    // Move the cursor.
    let dx = i32::from(keys.just_pressed(KeyCode::ArrowRight))
        - i32::from(keys.just_pressed(KeyCode::ArrowLeft));
    let dy = i32::from(keys.just_pressed(KeyCode::ArrowUp))
        - i32::from(keys.just_pressed(KeyCode::ArrowDown));
    if dx != 0 || dy != 0 {
        room.gx = (room.gx + dx).max(0); // unbounded grid
        room.gy = (room.gy + dy).max(0);
        changed = true;
    }

    // Grab / drop (reorder).
    if keys.just_pressed(KeyCode::KeyG) {
        match room.grab {
            None if occupied => {
                room.grab = Some(here);
                status = Some("grabbed — move and press G to drop".to_string());
            }
            Some(from) => {
                let dest = (room.gx, room.gy);
                if dest != from {
                    status = Some(swap_rooms(from, dest, &mut game_assets, &mut map_assets));
                }
                room.grab = None;
            }
            None => status = Some("nothing here to grab".to_string()),
        }
        changed = true;
    }

    // Edit the selected room.
    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::Space) {
        if occupied {
            *view = EditorView::Tiles;
            *buffer = load_buffer(&name_at(here), &game_assets, &map_assets);
            redraw(&mut commands, &overlay);
            draw_tiles(&mut commands, &buffer, &game_assets, &rock, center);
            return;
        }
        status = Some("empty — press A to add a room".to_string());
        changed = true;
    }

    // Add / delete.
    if keys.just_pressed(KeyCode::KeyA) && !occupied {
        status = Some(add_room(here, &mut game_assets, &mut map_assets));
        changed = true;
    }
    if keys.just_pressed(KeyCode::KeyD) && occupied {
        status = Some(delete_room(here, &mut game_assets, &mut map_assets));
        room.grab = None;
        changed = true;
    }

    // Reset to the default 12 (press twice).
    if keys.just_pressed(KeyCode::KeyR) {
        if room.confirm_reset {
            room.confirm_reset = false;
            status = Some(apply_world(
                default_rooms(),
                &mut game_assets,
                &mut map_assets,
            ));
        } else {
            room.confirm_reset = true;
            status = Some("press R again to reset to the default 12 rooms".to_string());
        }
        changed = true;
    } else if dx != 0 || dy != 0 || keys.just_pressed(KeyCode::KeyA) {
        room.confirm_reset = false;
    }

    if let Some(message) = status {
        room.status = message;
    }
    if changed {
        redraw(&mut commands, &overlay);
        draw_room_map(&mut commands, center, &game_assets, &map_assets, &room);
    }
}

fn draw_room_map(
    commands: &mut Commands,
    center: Vec2,
    assets: &GameAssets,
    maps: &Assets<MapData>,
    room: &RoomMap,
) {
    backdrop(commands, center);
    let cell = Vec2::new(GRID_W / VIEW_COLS as f32, GRID_H / VIEW_ROWS as f32);
    let bright = Color::srgb(0.92, 0.94, 0.98);
    let dim = Color::srgb(0.55, 0.58, 0.66);

    // A window of cells scrolls to keep the cursor roughly centred — the grid is
    // unbounded, so we only ever draw what's in view.
    let win = window_origin((room.gx, room.gy));
    let in_view = |cell: (i32, i32)| {
        (win.0..win.0 + VIEW_COLS).contains(&cell.0) && (win.1..win.1 + VIEW_ROWS).contains(&cell.1)
    };

    for sr in 0..VIEW_ROWS {
        for sc in 0..VIEW_COLS {
            let (gx, gy) = (win.0 + sc, win.1 + sr);
            let pos = cell_screen(center, gx, gy, win);
            let name = name_at((gx, gy));
            if let Some(map) = room_data(assets, maps, &name) {
                box_at(commands, pos, cell * 0.82, 151.0, lighten(map.bg));
                text_at(commands, pos, 13.0, bright, map.display_name(&name));
            } else {
                box_at(
                    commands,
                    pos,
                    cell * 0.72,
                    151.0,
                    Color::srgb(0.12, 0.12, 0.16),
                );
            }
        }
    }

    if let Some(source) = room.grab.filter(|s| in_view(*s)) {
        box_at(
            commands,
            cell_screen(center, source.0, source.1, win),
            cell * 0.9,
            150.6,
            Color::srgb(0.95, 0.78, 0.25),
        );
    }
    box_at(
        commands,
        cell_screen(center, room.gx, room.gy, win),
        cell * 0.96,
        150.5,
        Color::WHITE,
    );

    text_at(
        commands,
        center + Vec2::new(0.0, 200.0),
        24.0,
        bright,
        &format!("ROOMS   ·   at {}", name_at((room.gx, room.gy))),
    );
    text_at(
        commands,
        center + Vec2::new(0.0, 174.0),
        14.0,
        dim,
        &room.status,
    );
    text_at(
        commands,
        center + Vec2::new(0.0, -198.0),
        13.0,
        dim,
        "arrows move   ·   enter edit   ·   A add   ·   D delete",
    );
    text_at(
        commands,
        center + Vec2::new(0.0, -216.0),
        13.0,
        dim,
        "G grab / drop (reorder)   ·   R reset   ·   M back to tiles",
    );
}

// --- room operations -----------------------------------------------------

/// Read every registered room into an editable map.
fn current_world(assets: &GameAssets, maps: &Assets<MapData>) -> BTreeMap<String, MapData> {
    assets
        .room_names
        .iter()
        .filter_map(|name| Some((name.clone(), room_data(assets, maps, name)?.clone())))
        .collect()
}

fn add_room(at: (i32, i32), assets: &mut GameAssets, maps: &mut Assets<MapData>) -> String {
    let mut world = current_world(assets, maps);
    world.insert(name_at(at), standard_blank(PALETTE[0]));
    apply_world(world, assets, maps)
}

fn delete_room(at: (i32, i32), assets: &mut GameAssets, maps: &mut Assets<MapData>) -> String {
    let mut world = current_world(assets, maps);
    if world.len() <= 1 {
        return "can't delete the last room".to_string();
    }
    world.remove(&name_at(at));
    apply_world(world, assets, maps)
}

/// Move a room's contents to another cell, swapping if that cell is occupied.
fn swap_rooms(
    from: (i32, i32),
    to: (i32, i32),
    assets: &mut GameAssets,
    maps: &mut Assets<MapData>,
) -> String {
    let mut world = current_world(assets, maps);
    let (fname, tname) = (name_at(from), name_at(to));
    if let Some(from_map) = world.remove(&fname) {
        if let Some(to_map) = world.remove(&tname) {
            world.insert(fname, to_map);
            world.insert(tname, from_map);
        } else {
            world.insert(tname, from_map);
        }
    }
    apply_world(world, assets, maps)
}

/// Relink every room from grid adjacency, then write all files and refresh the
/// live assets (removing any rooms no longer present).
fn apply_world(
    mut world: BTreeMap<String, MapData>,
    assets: &mut GameAssets,
    maps: &mut Assets<MapData>,
) -> String {
    relink(&mut world);

    let removed: Vec<String> = assets
        .room_names
        .iter()
        .filter(|name| !world.contains_key(*name))
        .cloned()
        .collect();
    for name in removed {
        let _ = std::fs::remove_file(map_fs_path(&name));
        assets.maps.remove(&name);
    }

    let mut errors = 0;
    for (name, map) in &world {
        if std::fs::write(map_fs_path(name), map.to_ron()).is_err() {
            errors += 1;
            continue;
        }
        let id = assets.maps.get(name).map(Handle::id);
        if !matches!(id, Some(id) if maps.get_mut(id).map(|mut slot| *slot = map.clone()).is_some())
        {
            let handle = maps.add(map.clone());
            assets.maps.insert(name.clone(), handle);
        }
    }
    assets.room_names = world.keys().cloned().collect();

    if errors == 0 {
        format!("{} rooms saved", world.len())
    } else {
        format!("{errors} files could not be written")
    }
}

fn relink(world: &mut BTreeMap<String, MapData>) {
    let occupied: HashSet<(i32, i32)> = world.keys().filter_map(|name| parse_pos(name)).collect();
    let link = |gx: i32, gy: i32| {
        if gx >= 0 && gy >= 0 && occupied.contains(&(gx, gy)) {
            name_at((gx, gy))
        } else {
            String::new()
        }
    };
    for (name, map) in world.iter_mut() {
        if let Some((gx, gy)) = parse_pos(name) {
            map.north = link(gx, gy + 1);
            map.south = link(gx, gy - 1);
            map.east = link(gx + 1, gy);
            map.west = link(gx - 1, gy);
        }
        cut_doors(map);
    }
}

/// Open or seal a standard (40×22) room's doors to match its neighbours, so a
/// link is also a physical passage. Custom-sized rooms are left to the user.
fn cut_doors(map: &mut MapData) {
    const SHAFT: usize = 9;
    let h = map.tiles.len();
    let w = map
        .tiles
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(0);
    if w != 40 || h != 22 {
        return;
    }
    let mut grid: Vec<Vec<char>> = map.tiles.iter().map(|r| r.chars().collect()).collect();
    let door = |open: bool| if open { '.' } else { '#' };
    for r in [h - 3, h - 2] {
        grid[r][0] = door(!map.west.is_empty());
        grid[r][w - 1] = door(!map.east.is_empty());
    }
    for c in [SHAFT, SHAFT + 1] {
        grid[0][c] = door(!map.north.is_empty());
        grid[h - 1][c] = door(!map.south.is_empty());
    }
    map.tiles = grid.into_iter().map(|r| r.into_iter().collect()).collect();
}

fn save_tiles(buffer: &EditBuffer, assets: &mut GameAssets, maps: &mut Assets<MapData>) -> String {
    let map = map_from_buffer(buffer);
    if std::fs::write(map_fs_path(&buffer.name), map.to_ron()).is_err() {
        return "save failed".to_string();
    }
    let id = assets.maps.get(&buffer.name).map(Handle::id);
    if !matches!(id, Some(id) if maps.get_mut(id).map(|mut slot| *slot = map.clone()).is_some()) {
        let handle = maps.add(map);
        assets.maps.insert(buffer.name.clone(), handle);
        if !assets.room_names.contains(&buffer.name) {
            assets.room_names.push(buffer.name.clone());
            assets.room_names.sort();
        }
    }
    format!("saved {}", buffer.name)
}

// --- default rooms (porting the offline generator) -----------------------

/// The default 12-room world, regenerated in code for the Reset button.
fn default_rooms() -> BTreeMap<String, MapData> {
    let mut world = BTreeMap::new();
    let mut i = 0;
    for gy in 0..3 {
        for gx in 0..4 {
            world.insert(name_at((gx, gy)), default_room(gx, gy, bg_hsv(i)));
            i += 1;
        }
    }
    relink(&mut world);
    world
}

/// The shared 40×22 room shell: border, central shaft, and climbing ledges.
/// Doors are cut later by [`cut_doors`] from each room's neighbours.
fn standard_base() -> Vec<Vec<char>> {
    const W: usize = 40;
    const H: usize = 22;
    const SHAFT: usize = 9;
    let ledge_rows = [18usize, 15, 12, 9, 6, 3];

    let mut g = vec![vec!['.'; W]; H];
    g[0] = vec!['#'; W];
    g[H - 1] = vec!['#'; W];
    for row in &mut g {
        row[0] = '#';
        row[W - 1] = '#';
    }
    for (i, &rr) in ledge_rows.iter().enumerate() {
        let cols = if i % 2 == 0 {
            [SHAFT - 3, SHAFT - 2, SHAFT - 1]
        } else {
            [SHAFT + 2, SHAFT + 3, SHAFT + 4]
        };
        for c in cols {
            g[rr][c] = '#';
        }
    }
    for row in g.iter_mut().take(H - 1).skip(1) {
        row[SHAFT] = '.';
        row[SHAFT + 1] = '.';
    }
    g
}

fn standard_map(bg: [f32; 3], grid: Vec<Vec<char>>) -> MapData {
    MapData {
        name: String::new(),
        solid: "#".to_string(),
        spikes: "^".to_string(),
        rocks: "R".to_string(),
        north: String::new(),
        south: String::new(),
        east: String::new(),
        west: String::new(),
        teleports: Vec::new(),
        bg,
        tiles: grid
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect(),
    }
}

/// A blank, full-size room added from the room map (doors filled in by relink).
fn standard_blank(bg: [f32; 3]) -> MapData {
    standard_map(bg, standard_base())
}

fn default_room(gx: i32, gy: i32, bg: [f32; 3]) -> MapData {
    const H: usize = 22;
    let mut g = standard_base();
    if gx == 0 && gy == 0 {
        g[H - 3][3] = START_MARKER;
    } else {
        if gx == 0 {
            for c in [2, 3, 4] {
                g[H - 2][c] = '^';
            }
        }
        if gx == 3 {
            for c in [33, 34, 35] {
                g[H - 2][c] = '^';
            }
            g[1][34] = 'R';
        }
        if (gx + gy) % 2 == 0 {
            g[1][20] = 'R';
        }
    }
    standard_map(bg, g)
}

fn blank_map(bg: [f32; 3]) -> MapData {
    let (w, h) = (20usize, 14usize);
    let mut g = vec![vec!['.'; w]; h];
    g[0] = vec!['#'; w];
    g[h - 1] = vec!['#'; w];
    for row in &mut g {
        row[0] = '#';
        row[w - 1] = '#';
    }
    MapData {
        name: String::new(),
        solid: "#".to_string(),
        spikes: "^".to_string(),
        rocks: "R".to_string(),
        north: String::new(),
        south: String::new(),
        east: String::new(),
        west: String::new(),
        teleports: Vec::new(),
        bg,
        tiles: g.into_iter().map(|row| row.into_iter().collect()).collect(),
    }
}

/// HSV→RGB, matching the offline generator's `hsv(i/12, 0.5, 0.32)`.
fn bg_hsv(i: i32) -> [f32; 3] {
    let (h, s, v) = (i as f32 / 12.0, 0.5, 0.32);
    let c = v * s;
    let h6 = (h * 6.0).rem_euclid(6.0);
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r, g, b) = match h6 as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [round3(r + m), round3(g + m), round3(b + m)]
}

fn round3(v: f32) -> f32 {
    (v * 1000.0).round() / 1000.0
}

// --- shared helpers ------------------------------------------------------

fn load_buffer(name: &str, assets: &GameAssets, maps: &Assets<MapData>) -> EditBuffer {
    match room_data(assets, maps, name) {
        Some(map) => buffer_from_map(name, map),
        None => {
            let mut buffer = EditBuffer {
                name: name.to_string(),
                status: format!("{name} not loaded"),
                ..default()
            };
            buffer.grid = blank_map(PALETTE[0])
                .tiles
                .iter()
                .map(|r| r.chars().collect())
                .collect();
            buffer
        }
    }
}

fn buffer_from_map(name: &str, map: &MapData) -> EditBuffer {
    let width = map
        .tiles
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(0);
    let grid = map
        .tiles
        .iter()
        .map(|line| {
            let mut row: Vec<char> = line.chars().collect();
            row.resize(width, '.');
            row
        })
        .collect();
    EditBuffer {
        name: name.to_string(),
        display: map.name.clone(),
        grid,
        north: map.north.clone(),
        south: map.south.clone(),
        east: map.east.clone(),
        west: map.west.clone(),
        teleports: map.teleports.clone(),
        bg: map.bg,
        status: format!("editing {}", map.display_name(name)),
        ..default()
    }
}

fn map_from_buffer(buffer: &EditBuffer) -> MapData {
    MapData {
        name: buffer.display.clone(),
        solid: "#".to_string(),
        spikes: "^".to_string(),
        rocks: "R".to_string(),
        north: buffer.north.clone(),
        south: buffer.south.clone(),
        east: buffer.east.clone(),
        west: buffer.west.clone(),
        teleports: buffer.teleports.clone(),
        bg: buffer.bg,
        tiles: buffer
            .grid
            .iter()
            .map(|row| row.iter().collect::<String>())
            .collect(),
    }
}

fn room_data<'a>(
    assets: &GameAssets,
    maps: &'a Assets<MapData>,
    name: &str,
) -> Option<&'a MapData> {
    assets.maps.get(name).and_then(|h| maps.get(h))
}

fn sprite_for(ch: char, assets: &GameAssets, rock: &RockSprite) -> Option<Handle<Image>> {
    match ch {
        '#' => Some(assets.tile.clone()),
        '^' => Some(assets.spikes.clone()),
        'R' => Some(rock.0.clone()),
        c if c == START_MARKER => Some(assets.player.clone()),
        _ => None,
    }
}

fn name_at((gx, gy): (i32, i32)) -> String {
    format!("r{gx}_{gy}")
}

/// Outcome of feeding typed keys to a text field.
enum Typing {
    Continue,
    Confirm,
    Cancel,
}

/// Apply typed keys to `text` (max ~28 chars), reporting confirm/cancel.
fn apply_typing(text: &mut String, events: &[KeyboardInput]) -> Typing {
    for ev in events {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        match &ev.logical_key {
            Key::Enter => return Typing::Confirm,
            Key::Escape => return Typing::Cancel,
            Key::Backspace => {
                text.pop();
            }
            Key::Space if text.len() < 28 => text.push(' '),
            Key::Character(s) => {
                for c in s.chars() {
                    if !c.is_control() && text.len() < 28 {
                        text.push(c);
                    }
                }
            }
            _ => {}
        }
    }
    Typing::Continue
}

/// Top-left grid cell of the scrolling window, keeping the cursor centred.
fn window_origin((gx, gy): (i32, i32)) -> (i32, i32) {
    ((gx - VIEW_COLS / 2).max(0), (gy - VIEW_ROWS / 2).max(0))
}

/// Screen position of a grid cell, given the window's top-left cell.
fn cell_screen(center: Vec2, gx: i32, gy: i32, win: (i32, i32)) -> Vec2 {
    let cell = Vec2::new(GRID_W / VIEW_COLS as f32, GRID_H / VIEW_ROWS as f32);
    Vec2::new(
        center.x - GRID_W / 2.0 + cell.x * ((gx - win.0) as f32 + 0.5),
        center.y - GRID_H / 2.0 + cell.y * ((gy - win.1) as f32 + 0.5),
    )
}

/// Parse a `r{col}_{row}` room name into its grid position (multi-digit).
fn parse_pos(name: &str) -> Option<(i32, i32)> {
    let (col, row) = name.strip_prefix('r')?.split_once('_')?;
    Some((col.parse().ok()?, row.parse().ok()?))
}

fn redraw(commands: &mut Commands, overlay: &Query<Entity, With<EditorEntity>>) {
    for entity in overlay {
        commands.entity(entity).despawn();
    }
}

fn backdrop(commands: &mut Commands, center: Vec2) {
    box_at(
        commands,
        center,
        Vec2::new(960.0, 540.0),
        150.0,
        Color::srgba(0.02, 0.02, 0.05, 0.99),
    );
}

fn box_at(commands: &mut Commands, pos: Vec2, size: Vec2, z: f32, color: Color) {
    commands.spawn((
        EditorEntity,
        Sprite {
            color,
            custom_size: Some(size),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, z),
    ));
}

fn text_at(commands: &mut Commands, pos: Vec2, size: f32, color: Color, text: &str) {
    commands.spawn((
        EditorEntity,
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        Transform::from_xyz(pos.x, pos.y, 156.0),
    ));
}

fn lighten(bg: [f32; 3]) -> Color {
    Color::srgb(
        (bg[0] * 1.7 + 0.06).min(1.0),
        (bg[1] * 1.7 + 0.06).min(1.0),
        (bg[2] * 1.7 + 0.06).min(1.0),
    )
}

fn camera_center(camera: &Query<&Transform, With<Camera2d>>) -> Vec2 {
    camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Reset generator must produce 12 internally-consistent rooms: links
    /// reference real rooms, and each room's doors match its links.
    #[test]
    fn default_rooms_are_consistent() {
        let world = default_rooms();
        assert_eq!(world.len(), 12);

        for (name, map) in &world {
            for link in [&map.north, &map.south, &map.east, &map.west] {
                if !link.is_empty() {
                    assert!(world.contains_key(link), "{name} links to missing {link}");
                }
            }
        }

        // The bottom-left start room opens north + east only, and holds the marker.
        let start = &world["r0_0"];
        assert!(start.tiles.iter().any(|r| r.contains(START_MARKER)));
        assert!(!start.north.is_empty() && !start.east.is_empty());
        assert!(start.south.is_empty() && start.west.is_empty());
        // The shaft's ceiling gap is open (north door) and the floor is sealed.
        assert_eq!(start.tiles[0].chars().nth(9), Some('.'));
        assert_eq!(start.tiles[21].chars().nth(9), Some('#'));
    }
}
