//! Main menu and pause menu.
//!
//! The game boots into [`GameState::MainMenu`] (Start / Quit). "Start" kicks off
//! asset loading and then play. During play, `Esc` (or the gamepad `Select`
//! button) toggles a [`Paused`] overlay (Continue / Quit); gameplay is frozen
//! while paused (the gameplay [`GameSet`](crate::GameSet) chain is gated on
//! [`Paused::Running`]).
//!
//! Both menus are drawn the same lightweight way as the world map — a backdrop
//! sprite plus `Text2d` entries around the camera — and navigated with the
//! arrows / D-pad and confirmed with jump / `Enter` / `South`.

use bevy::app::AppExit;
use bevy::prelude::*;

use crate::state::GameState;
use crate::worldmap::MapView;

/// Whether gameplay is paused. Gameplay runs only when `Running`.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Paused {
    #[default]
    Running,
    Paused,
}

/// Tags every entity that makes up a menu (despawned when it closes).
#[derive(Component)]
struct MenuEntity;

/// One selectable menu row, by index from the top.
#[derive(Component)]
struct MenuItem(usize);

/// The highlighted row, shared between the two menus (reset when one opens).
#[derive(Resource, Default)]
struct MenuCursor(usize);

/// What a menu row does when chosen.
#[derive(Clone, Copy)]
enum MenuAction {
    Start,
    Continue,
    Quit,
    /// Jump into the level builder — only offered in debug builds.
    #[cfg(debug_assertions)]
    OpenEditor,
}

fn main_menu_items() -> Vec<(&'static str, MenuAction)> {
    let mut items = vec![("Start", MenuAction::Start)];
    #[cfg(debug_assertions)]
    items.push(("Level Builder", MenuAction::OpenEditor));
    items.push(("Quit", MenuAction::Quit));
    items
}

fn pause_menu_items() -> Vec<(&'static str, MenuAction)> {
    let mut items = vec![("Continue", MenuAction::Continue)];
    #[cfg(debug_assertions)]
    items.push(("Level Builder", MenuAction::OpenEditor));
    items.push(("Quit", MenuAction::Quit));
    items
}

fn labels(items: &[(&'static str, MenuAction)]) -> Vec<&'static str> {
    items.iter().map(|(label, _)| *label).collect()
}

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Paused>()
            .init_resource::<MenuCursor>()
            .add_systems(OnEnter(GameState::MainMenu), spawn_main_menu)
            .add_systems(OnExit(GameState::MainMenu), despawn_menu)
            .add_systems(
                Update,
                main_menu_update.run_if(in_state(GameState::MainMenu)),
            )
            .add_systems(
                Update,
                toggle_pause
                    .run_if(in_state(GameState::Playing).and_then(in_state(MapView::Closed))),
            )
            .add_systems(OnEnter(Paused::Paused), spawn_pause_menu)
            .add_systems(OnExit(Paused::Paused), despawn_menu)
            .add_systems(Update, pause_menu_update.run_if(in_state(Paused::Paused)));
    }
}

fn spawn_main_menu(
    mut commands: Commands,
    mut cursor: ResMut<MenuCursor>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    cursor.0 = 0;
    draw_menu(
        &mut commands,
        camera_center(&camera),
        "PLATFORMER",
        &labels(&main_menu_items()),
    );
}

fn spawn_pause_menu(
    mut commands: Commands,
    mut cursor: ResMut<MenuCursor>,
    camera: Query<&Transform, With<Camera2d>>,
) {
    cursor.0 = 0;
    draw_menu(
        &mut commands,
        camera_center(&camera),
        "PAUSED",
        &labels(&pause_menu_items()),
    );
}

fn despawn_menu(mut commands: Commands, menu: Query<Entity, With<MenuEntity>>) {
    for entity in &menu {
        commands.entity(entity).despawn();
    }
}

fn toggle_pause(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    state: Res<State<Paused>>,
    mut next: ResMut<NextState<Paused>>,
) {
    let pressed = keys.just_pressed(KeyCode::Escape)
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::Select));
    if pressed {
        next.set(match state.get() {
            Paused::Running => Paused::Paused,
            Paused::Paused => Paused::Running,
        });
    }
}

fn main_menu_update(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut cursor: ResMut<MenuCursor>,
    rows: Query<(&MenuItem, &mut TextColor)>,
    mut game_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    #[cfg(debug_assertions)] mut start_in_editor: ResMut<crate::editor::StartInEditor>,
) {
    let Some(choice) = update_menu(&keys, &gamepads, &mut cursor, rows) else {
        return;
    };
    match main_menu_items().get(choice).map(|(_, action)| *action) {
        Some(MenuAction::Start) => game_state.set(GameState::Loading),
        #[cfg(debug_assertions)]
        Some(MenuAction::OpenEditor) => {
            game_state.set(GameState::Loading); // load assets, then `editor` jumps in
            start_in_editor.0 = true;
        }
        Some(MenuAction::Quit) => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn pause_menu_update(
    keys: Res<ButtonInput<KeyCode>>,
    gamepads: Query<&Gamepad>,
    mut cursor: ResMut<MenuCursor>,
    rows: Query<(&MenuItem, &mut TextColor)>,
    mut next: ResMut<NextState<Paused>>,
    mut exit: MessageWriter<AppExit>,
    #[cfg(debug_assertions)] mut start_in_editor: ResMut<crate::editor::StartInEditor>,
) {
    let Some(choice) = update_menu(&keys, &gamepads, &mut cursor, rows) else {
        return;
    };
    match pause_menu_items().get(choice).map(|(_, action)| *action) {
        Some(MenuAction::Continue) => next.set(Paused::Running),
        #[cfg(debug_assertions)]
        Some(MenuAction::OpenEditor) => {
            next.set(Paused::Running); // resume, then `editor` jumps in
            start_in_editor.0 = true;
        }
        Some(MenuAction::Quit) => {
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

/// Move the cursor, recolour the rows, and return the chosen index on confirm.
fn update_menu(
    keys: &ButtonInput<KeyCode>,
    gamepads: &Query<&Gamepad>,
    cursor: &mut MenuCursor,
    mut items: Query<(&MenuItem, &mut TextColor)>,
) -> Option<usize> {
    let count = items.iter().count().max(1) as i32;
    let up = keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::DPadUp));
    let down = keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::DPadDown));
    let confirm = keys.any_just_pressed([KeyCode::Enter, KeyCode::Space, KeyCode::KeyZ])
        || gamepads
            .iter()
            .any(|g| g.just_pressed(GamepadButton::South));

    let delta = i32::from(down) - i32::from(up);
    if delta != 0 {
        cursor.0 = (cursor.0 as i32 + delta).rem_euclid(count) as usize;
    }

    for (item, mut color) in &mut items {
        *color = TextColor(if item.0 == cursor.0 {
            Color::srgb(1.0, 0.85, 0.3)
        } else {
            Color::srgb(0.62, 0.64, 0.72)
        });
    }

    confirm.then_some(cursor.0)
}

fn draw_menu(commands: &mut Commands, center: Vec2, title: &str, items: &[&str]) {
    commands.spawn((
        MenuEntity,
        Sprite {
            color: Color::srgba(0.03, 0.03, 0.06, 0.97),
            custom_size: Some(Vec2::new(960.0, 540.0)),
            ..default()
        },
        Transform::from_xyz(center.x, center.y, 200.0),
    ));
    commands.spawn((
        MenuEntity,
        Text2d::new(title),
        TextFont {
            font_size: FontSize::Px(46.0),
            ..default()
        },
        TextColor(Color::srgb(0.95, 0.96, 1.0)),
        Transform::from_xyz(center.x, center.y + 110.0, 201.0),
    ));
    for (i, label) in items.iter().enumerate() {
        commands.spawn((
            MenuEntity,
            MenuItem(i),
            Text2d::new(*label),
            TextFont {
                font_size: FontSize::Px(32.0),
                ..default()
            },
            TextColor(Color::srgb(0.62, 0.64, 0.72)),
            Transform::from_xyz(center.x, center.y - 10.0 - i as f32 * 58.0, 201.0),
        ));
    }
}

fn camera_center(camera: &Query<&Transform, With<Camera2d>>) -> Vec2 {
    camera
        .single()
        .map(|t| t.translation.truncate())
        .unwrap_or(Vec2::ZERO)
}
