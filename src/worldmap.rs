//! A pause-screen world map (toggle with `M` or the gamepad `Start` button).
//!
//! It shows every room "glued together" in a grid sized from the room names
//! (`r{col}_{row}`), so added rooms expand it — each drawn as a tiny minimap in its
//! own background colour and labelled with its display name, with the room you're
//! in highlighted.
//! Move the selection with the arrows / D-pad and press jump (`Space` / `South`)
//! to **zoom** the selected room to full detail; press it again to zoom back out.
//!
//! Gameplay is paused while the map is open (the gameplay [`GameSet`](crate::GameSet)
//! chain is gated on [`MapView::Closed`]). The overlay is drawn with plain sprites
//! placed around the frozen camera, on top of everything (high `z`).

use bevy::prelude::*;

use crate::menu::Paused;
use crate::state::GameState;
use crate::world::{CurrentRoom, GameAssets, MapData, START_MARKER};

/// Whether the world map overlay is showing. Gameplay runs only when `Closed`.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MapView {
    #[default]
    Closed,
    Open,
}

/// The currently highlighted room and whether we're zoomed into it.
#[derive(Resource, Default)]
struct MapCursor {
    gx: i32,
    gy: i32,
    zoomed: bool,
}

/// Tags every entity that makes up the overlay (despawned when it closes).
#[derive(Component)]
struct WorldMapEntity;

/// The movable selection outline in the overview.
#[derive(Component)]
struct CursorHighlight;

// Layout (in screen pixels; the camera is 1 unit = 1 pixel). The grid's column
// and row counts are derived from the room names, so new rooms expand it.
const GRID_W: f32 = 840.0;
const GRID_H: f32 = 372.0;

pub struct WorldMapPlugin;

impl Plugin for WorldMapPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<MapView>()
            .init_resource::<MapCursor>()
            .add_systems(
                Update,
                toggle_map.run_if(in_state(GameState::Playing).and_then(in_state(Paused::Running))),
            )
            .add_systems(OnEnter(MapView::Open), open_map)
            .add_systems(OnExit(MapView::Open), close_map)
            .add_systems(Update, navigate_map.run_if(in_state(MapView::Open)));
    }
}

fn toggle_map(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    state: Res<State<MapView>>,
    mut next: ResMut<NextState<MapView>>,
) {
    let pressed = keys.just_pressed(KeyCode::KeyM)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::Start));
    if pressed {
        next.set(match state.get() {
            MapView::Closed => MapView::Open,
            MapView::Open => MapView::Closed,
        });
    }
}

fn open_map(
    mut commands: Commands,
    mut cursor: ResMut<MapCursor>,
    current: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    maps: Res<Assets<MapData>>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    if let Some((gx, gy)) = parse_pos(&current.name) {
        cursor.gx = gx;
        cursor.gy = gy;
    }
    cursor.zoomed = false;
    let center = camera_center(&camera);
    draw_overview(
        &mut commands,
        center,
        &assets,
        &maps,
        &current.name,
        &cursor,
    );
}

fn close_map(
    mut commands: Commands,
    overlay: Query<Entity, With<WorldMapEntity>>,
    mut cursor: ResMut<MapCursor>,
) {
    for entity in &overlay {
        commands.entity(entity).despawn();
    }
    cursor.zoomed = false;
}

#[allow(clippy::too_many_arguments)] // a Bevy system; each param is a distinct query/resource
fn navigate_map(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut cursor: ResMut<MapCursor>,
    current: Res<CurrentRoom>,
    assets: Res<GameAssets>,
    maps: Res<Assets<MapData>>,
    camera: Query<&Transform, With<Camera2d>>,
    overlay: Query<Entity, With<WorldMapEntity>>,
    mut highlight: Query<&mut Transform, (With<CursorHighlight>, Without<Camera2d>)>,
) {
    let left = pressed_any(
        &keys,
        &[KeyCode::ArrowLeft, KeyCode::KeyA],
        &gamepads,
        GamepadButton::DPadLeft,
    );
    let right = pressed_any(
        &keys,
        &[KeyCode::ArrowRight, KeyCode::KeyD],
        &gamepads,
        GamepadButton::DPadRight,
    );
    let up = pressed_any(
        &keys,
        &[KeyCode::ArrowUp, KeyCode::KeyW],
        &gamepads,
        GamepadButton::DPadUp,
    );
    let down = pressed_any(
        &keys,
        &[KeyCode::ArrowDown, KeyCode::KeyS],
        &gamepads,
        GamepadButton::DPadDown,
    );
    let confirm = keys.just_pressed(KeyCode::Space)
        || keys.just_pressed(KeyCode::KeyZ)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::South));

    let (cols, rows) = grid_dims(&assets.room_names);
    let nx = (cursor.gx + i32::from(right) - i32::from(left)).clamp(0, cols - 1);
    let ny = (cursor.gy + i32::from(up) - i32::from(down)).clamp(0, rows - 1);
    let moved = (nx, ny) != (cursor.gx, cursor.gy);
    cursor.gx = nx;
    cursor.gy = ny;
    if confirm {
        cursor.zoomed = !cursor.zoomed;
    }

    let center = camera_center(&camera);

    if confirm || (moved && cursor.zoomed) {
        // Switching view (or zoomed room) → rebuild the overlay.
        for entity in &overlay {
            commands.entity(entity).despawn();
        }
        if cursor.zoomed {
            draw_zoom(&mut commands, center, &assets, &maps, &cursor);
        } else {
            draw_overview(
                &mut commands,
                center,
                &assets,
                &maps,
                &current.name,
                &cursor,
            );
        }
    } else if moved {
        // Just slide the selection outline — no need to redraw the thumbnails.
        if let Ok(mut transform) = highlight.single_mut() {
            let pos = cell_center(center, cursor.gx, cursor.gy, cols, rows);
            transform.translation.x = pos.x;
            transform.translation.y = pos.y;
        }
    }
}

// --- drawing -------------------------------------------------------------

fn draw_overview(
    commands: &mut Commands,
    center: Vec2,
    assets: &GameAssets,
    maps: &Assets<MapData>,
    current_name: &str,
    cursor: &MapCursor,
) {
    backdrop(commands, center);
    label(
        commands,
        center,
        Vec2::new(0.0, GRID_H / 2.0 + 36.0),
        30.0,
        "WORLD MAP",
    );
    label(
        commands,
        center,
        Vec2::new(0.0, -GRID_H / 2.0 - 34.0),
        17.0,
        "[M] close    move: arrows / d-pad    [jump] zoom",
    );

    let (cols, rows) = grid_dims(&assets.room_names);
    let cell = cell_size(cols, rows);

    // Highlight the room you're actually in (gold), then the selection (white).
    if let Some((gx, gy)) = parse_pos(current_name) {
        ring(
            commands,
            cell_center(center, gx, gy, cols, rows),
            cell * 0.97,
            90.4,
            Color::srgb(0.95, 0.78, 0.25),
            false,
        );
    }
    ring(
        commands,
        cell_center(center, cursor.gx, cursor.gy, cols, rows),
        cell * 0.9,
        90.6,
        Color::WHITE,
        true,
    );

    for name in &assets.room_names {
        let (Some((gx, gy)), Some(map)) = (parse_pos(name), room_data(assets, maps, name)) else {
            continue;
        };
        let pos = cell_center(center, gx, gy, cols, rows);
        draw_room(commands, pos, cell * 0.82, map, 91.0);
        // Display name (or key) along the bottom of each thumbnail.
        label(
            commands,
            center,
            pos - center + Vec2::new(0.0, -cell.y * 0.36),
            12.0,
            map.display_name(name),
        );
    }
}

fn draw_zoom(
    commands: &mut Commands,
    center: Vec2,
    assets: &GameAssets,
    maps: &Assets<MapData>,
    cursor: &MapCursor,
) {
    backdrop(commands, center);
    let key = format!("r{}_{}", cursor.gx, cursor.gy);
    if let Some(map) = room_data(assets, maps, &key) {
        label(
            commands,
            center,
            Vec2::new(0.0, 220.0),
            26.0,
            map.display_name(&key),
        );
        label(commands, center, Vec2::new(0.0, 196.0), 14.0, &key);
        draw_room(commands, center, Vec2::new(760.0, 392.0), map, 91.0);
    } else {
        label(commands, center, Vec2::new(0.0, 216.0), 26.0, &key);
        label(commands, center, Vec2::ZERO, 20.0, "· empty ·");
    }
    label(
        commands,
        center,
        Vec2::new(0.0, -216.0),
        17.0,
        "[jump] back to map    [M] close",
    );
}

/// Draw one room (background + its solid/spike/rock/start tiles), scaled to fit
/// the `max` box at `center`.
fn draw_room(commands: &mut Commands, center: Vec2, max: Vec2, map: &MapData, z: f32) {
    let w = map
        .tiles
        .iter()
        .map(|r| r.chars().count())
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let h = map.tiles.len().max(1) as f32;
    let tile = (max.x / w).min(max.y / h);
    let (room_w, room_h) = (w * tile, h * tile);

    commands.spawn((
        WorldMapEntity,
        Sprite {
            color: lighten(map.bg),
            custom_size: Some(Vec2::new(room_w, room_h)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, z),
    ));

    for (r, line) in map.tiles.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            let Some(color) = tile_color(ch, map) else {
                continue;
            };
            let x = center.x - room_w / 2.0 + (c as f32 + 0.5) * tile;
            let y = center.y + room_h / 2.0 - (r as f32 + 0.5) * tile;
            commands.spawn((
                WorldMapEntity,
                Sprite {
                    color,
                    custom_size: Some(Vec2::splat(tile * 0.92)),
                    ..default()
                },
                Transform::from_xyz(x, y, z + 1.0),
            ));
        }
    }
}

fn backdrop(commands: &mut Commands, center: Vec2) {
    commands.spawn((
        WorldMapEntity,
        Sprite {
            color: Color::srgba(0.02, 0.02, 0.05, 0.93),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 90.0),
    ));
}

/// A coloured box drawn behind a cell (so its border peeks out around the
/// thumbnail). Optionally tags it so the selection ring can be moved.
fn ring(commands: &mut Commands, pos: Vec2, size: Vec2, z: f32, color: Color, selection: bool) {
    let mut entity = commands.spawn((
        WorldMapEntity,
        Sprite {
            color,
            custom_size: Some(size),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, z),
    ));
    if selection {
        entity.insert(CursorHighlight);
    }
}

fn label(commands: &mut Commands, center: Vec2, offset: Vec2, size: f32, text: &str) {
    commands.spawn((
        WorldMapEntity,
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.92, 0.96)),
        Transform::from_xyz(center.x + offset.x, center.y + offset.y, 95.0),
    ));
}

// --- helpers -------------------------------------------------------------

fn camera_center(camera: &Query<&Transform, With<Camera2d>>) -> Vec2 {
    camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO)
}

/// Grid dimensions implied by the room names (`r{col}{row}`), at least 1×1.
fn grid_dims(names: &[String]) -> (i32, i32) {
    let (mut cols, mut rows) = (1, 1);
    for name in names {
        if let Some((gx, gy)) = parse_pos(name) {
            cols = cols.max(gx + 1);
            rows = rows.max(gy + 1);
        }
    }
    (cols, rows)
}

fn cell_size(cols: i32, rows: i32) -> Vec2 {
    Vec2::new(GRID_W / cols as f32, GRID_H / rows as f32)
}

fn cell_center(center: Vec2, gx: i32, gy: i32, cols: i32, rows: i32) -> Vec2 {
    let cell = cell_size(cols, rows);
    Vec2::new(
        center.x - GRID_W / 2.0 + cell.x * (gx as f32 + 0.5),
        center.y - GRID_H / 2.0 + cell.y * (gy as f32 + 0.5),
    )
}

/// True if any of `codes` was just pressed, or the gamepad `button` was.
fn pressed_any(
    keys: &ButtonInput<KeyCode>,
    codes: &[KeyCode],
    gamepads: &Query<&Gamepad>,
    button: GamepadButton,
) -> bool {
    keys.any_just_pressed(codes.iter().copied()) || gamepads.iter().any(|g| g.just_pressed(button))
}

/// Parse a `r{col}_{row}` room name into its grid position (multi-digit).
fn parse_pos(name: &str) -> Option<(i32, i32)> {
    let (col, row) = name.strip_prefix('r')?.split_once('_')?;
    Some((col.parse().ok()?, row.parse().ok()?))
}

fn room_data<'a>(
    assets: &GameAssets,
    maps: &'a Assets<MapData>,
    name: &str,
) -> Option<&'a MapData> {
    assets.maps.get(name).and_then(|h| maps.get(h))
}

fn tile_color(ch: char, map: &MapData) -> Option<Color> {
    if ch == START_MARKER {
        Some(Color::srgb(0.4, 0.9, 0.5))
    } else if map.solid.contains(ch) {
        Some(Color::srgb(0.78, 0.82, 0.9))
    } else if map.spikes.contains(ch) {
        Some(Color::srgb(0.9, 0.3, 0.3))
    } else if map.rocks.contains(ch) {
        Some(Color::srgb(0.85, 0.6, 0.3))
    } else {
        None
    }
}

/// Brighten a room's (deliberately dark) background so it reads on the map.
fn lighten(bg: [f32; 3]) -> Color {
    Color::srgb(
        (bg[0] * 1.7 + 0.06).min(1.0),
        (bg[1] * 1.7 + 0.06).min(1.0),
        (bg[2] * 1.7 + 0.06).min(1.0),
    )
}
