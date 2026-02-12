use bevy::prelude::*;

use breakpoint_platformer::course_gen::{COURSE_HEIGHT, COURSE_WIDTH, Course, Tile};
use breakpoint_platformer::physics::TILE_SIZE;

use crate::app::AppState;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Editor), setup_editor)
            .add_systems(
                Update,
                (
                    editor_palette_system,
                    editor_place_tile_system,
                    editor_tile_render_system,
                    editor_exit_system,
                )
                    .run_if(in_state(AppState::Editor)),
            )
            .add_systems(OnExit(AppState::Editor), cleanup_editor);
    }
}

/// The editor grid state.
#[derive(Resource)]
struct EditorState {
    tiles: Vec<Tile>,
    width: usize,
    height: usize,
    selected_tile: Tile,
}

impl EditorState {
    fn new() -> Self {
        let width = COURSE_WIDTH;
        let height = COURSE_HEIGHT;
        let mut tiles = vec![Tile::Empty; width * height];

        // Fill bottom row with Solid
        for tile in tiles.iter_mut().take(width) {
            *tile = Tile::Solid;
        }

        Self {
            tiles,
            width,
            height,
            selected_tile: Tile::Solid,
        }
    }

    fn get(&self, x: usize, y: usize) -> Tile {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x]
        } else {
            Tile::Empty
        }
    }

    fn set(&mut self, x: usize, y: usize, tile: Tile) {
        if x < self.width && y < self.height {
            self.tiles[y * self.width + x] = tile;
        }
    }

    /// Export as a Course struct for use in games.
    #[allow(dead_code)]
    fn to_course(&self) -> Course {
        Course {
            width: self.width as u32,
            height: self.height as u32,
            tiles: self.tiles.clone(),
            spawn_x: 2.0,
            spawn_y: 3.0,
        }
    }
}

/// Marker for editor UI entities.
#[derive(Component)]
struct EditorUi;

/// Marker for the tile grid visualization entities.
#[derive(Component)]
struct EditorTileEntity {
    x: usize,
    y: usize,
}

/// Marker for the selected tile indicator text.
#[derive(Component)]
struct SelectedTileText;

/// Palette button component.
#[derive(Component)]
struct PaletteButton(Tile);

const PALETTE_TILES: &[(Tile, &str, [f32; 3])] = &[
    (Tile::Empty, "Erase", [0.15, 0.15, 0.15]),
    (Tile::Solid, "Solid", [0.4, 0.4, 0.5]),
    (Tile::Platform, "Platform", [0.3, 0.6, 0.3]),
    (Tile::Hazard, "Hazard", [0.9, 0.2, 0.1]),
    (Tile::Checkpoint, "Checkpoint", [0.2, 0.5, 0.9]),
    (Tile::Finish, "Finish", [1.0, 0.85, 0.1]),
];

#[allow(clippy::too_many_arguments)]
fn setup_editor(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(EditorState::new());

    let tile_mesh = meshes.add(Cuboid::new(
        TILE_SIZE * 0.95,
        TILE_SIZE * 0.95,
        TILE_SIZE * 0.1,
    ));
    let empty_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.15, 0.15, 0.15, 0.3),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    // Spawn tile grid entities
    for y in 0..COURSE_HEIGHT {
        for x in 0..COURSE_WIDTH {
            let wx = x as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            let wy = y as f32 * TILE_SIZE + TILE_SIZE / 2.0;

            commands.spawn((
                EditorUi,
                EditorTileEntity { x, y },
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(empty_mat.clone()),
                Transform::from_xyz(wx, wy, 0.0),
            ));
        }
    }

    // Camera for editor (side view)
    commands.spawn((
        EditorUi,
        Camera3d::default(),
        Transform::from_xyz(
            (COURSE_WIDTH as f32 * TILE_SIZE) / 2.0,
            (COURSE_HEIGHT as f32 * TILE_SIZE) / 2.0,
            50.0,
        )
        .looking_at(
            Vec3::new(
                (COURSE_WIDTH as f32 * TILE_SIZE) / 2.0,
                (COURSE_HEIGHT as f32 * TILE_SIZE) / 2.0,
                0.0,
            ),
            Vec3::Y,
        ),
    ));

    // HUD: palette and info
    let bg_color = Color::srgba(0.08, 0.08, 0.12, 0.9);
    let text_color = Color::srgb(0.9, 0.9, 0.9);

    commands
        .spawn((
            EditorUi,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(bg_color),
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("Course Editor"),
                TextFont {
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::srgb(0.3, 0.7, 1.0)),
            ));

            parent.spawn((
                SelectedTileText,
                Text::new("Selected: Solid"),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(text_color),
            ));

            parent.spawn((
                Text::new("Click to place | Esc to exit"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.6, 0.6, 0.6)),
            ));

            // Palette buttons
            for &(tile, label, color) in PALETTE_TILES {
                parent
                    .spawn((
                        PaletteButton(tile),
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(color[0], color[1], color[2])),
                    ))
                    .with_child((
                        Text::new(label),
                        TextFont {
                            font_size: 14.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
            }
        });
}

fn editor_palette_system(
    interaction_query: Query<(&Interaction, &PaletteButton), Changed<Interaction>>,
    mut editor: ResMut<EditorState>,
    mut text_query: Query<&mut Text, With<SelectedTileText>>,
) {
    for (interaction, btn) in &interaction_query {
        if *interaction == Interaction::Pressed {
            editor.selected_tile = btn.0;
            if let Ok(mut text) = text_query.single_mut() {
                let name = PALETTE_TILES
                    .iter()
                    .find(|(t, _, _)| *t == btn.0)
                    .map(|(_, n, _)| *n)
                    .unwrap_or("?");
                **text = format!("Selected: {name}");
            }
        }
    }
}

fn editor_place_tile_system(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut editor: ResMut<EditorState>,
) {
    if !mouse.pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_transform)) = cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_transform, cursor_pos) else {
        return;
    };

    // Raycast to Z=0 plane
    if ray.direction.z.abs() < 1e-6 {
        return;
    }
    let t = -ray.origin.z / ray.direction.z;
    let point = ray.origin + ray.direction * t;

    let tile_x = (point.x / TILE_SIZE).floor() as i32;
    let tile_y = (point.y / TILE_SIZE).floor() as i32;

    if tile_x >= 0
        && tile_y >= 0
        && (tile_x as usize) < editor.width
        && (tile_y as usize) < editor.height
    {
        let tile = editor.selected_tile;
        editor.set(tile_x as usize, tile_y as usize, tile);
    }
}

fn editor_tile_render_system(
    editor: Res<EditorState>,
    mut tile_query: Query<(&EditorTileEntity, &mut MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if !editor.is_changed() {
        return;
    }

    for (entity, mut mat_handle) in &mut tile_query {
        let tile = editor.get(entity.x, entity.y);
        let color = match tile {
            Tile::Empty => Color::srgba(0.15, 0.15, 0.15, 0.3),
            Tile::Solid => Color::srgb(0.4, 0.4, 0.5),
            Tile::Platform => Color::srgb(0.3, 0.6, 0.3),
            Tile::Hazard => Color::srgb(0.9, 0.2, 0.1),
            Tile::Checkpoint => Color::srgb(0.2, 0.5, 0.9),
            Tile::Finish => Color::srgb(1.0, 0.85, 0.1),
            Tile::PowerUpSpawn => Color::srgb(0.8, 0.4, 0.8),
        };

        let alpha_mode = if tile == Tile::Empty {
            AlphaMode::Blend
        } else {
            AlphaMode::Opaque
        };

        mat_handle.0 = materials.add(StandardMaterial {
            base_color: color,
            alpha_mode,
            unlit: true,
            ..default()
        });
    }
}

fn editor_exit_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::Lobby);
    }
}

fn cleanup_editor(mut commands: Commands, query: Query<Entity, With<EditorUi>>) {
    for entity in &query {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<EditorState>();
}
