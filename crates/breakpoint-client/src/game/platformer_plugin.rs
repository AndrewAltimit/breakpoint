use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::NonSend;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;

use breakpoint_core::game_trait::{GameId, PlayerId};
use breakpoint_platformer::course_gen::Tile;
use breakpoint_platformer::physics::{PLAYER_HEIGHT, PLAYER_WIDTH, PlatformerInput, TILE_SIZE};
use breakpoint_platformer::{PlatformRacer, PlatformerState};

use crate::app::AppState;
use crate::net_client::WsClient;
use crate::theme::{Theme, rgb, rgba};

use super::{
    ActiveGame, ControlsHint, GameEntity, GameRegistry, HudPosition, NetworkRole,
    player_color_to_bevy, read_game_state, send_player_input, spawn_hud_text,
};

pub struct PlatformerPlugin;

impl Plugin for PlatformerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_platformer)
            .add_systems(
                Update,
                (
                    setup_platformer.run_if(platformer_needs_setup),
                    ApplyDeferred,
                    (
                        platformer_input_system,
                        platformer_render_sync,
                        platformer_hud_system,
                    ),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame).and(is_platformer_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_platformer.run_if(resource_exists::<PlatformerLocalInput>),
            );
    }
}

fn register_platformer(mut registry: ResMut<GameRegistry>) {
    registry.register(GameId::Platformer, || Box::new(PlatformRacer::new()));
}

fn is_platformer_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == GameId::Platformer)
}

fn platformer_needs_setup(input: Option<Res<PlatformerLocalInput>>) -> bool {
    input.is_none()
}

/// Local input state for platformer.
#[derive(Resource, Default)]
struct PlatformerLocalInput {
    move_dir: f32,
    jump: bool,
}

/// Marker for player mesh entities.
#[derive(Component)]
struct PlatformerPlayerEntity(PlayerId);

/// Marker for course tile entities.
#[derive(Component)]
struct CourseTileEntity;

/// Marker for HUD timer text.
#[derive(Component)]
struct PlatformerTimerText;

/// Marker for position indicator text.
#[derive(Component)]
struct PlatformerPositionText;

fn setup_platformer(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lobby: Res<crate::lobby::LobbyState>,
    network_role: Res<NetworkRole>,
    active_game: Res<ActiveGame>,
    theme: Res<Theme>,
) {
    commands.insert_resource(PlatformerLocalInput::default());

    // Get course from game state
    let state: Option<PlatformerState> = read_game_state(&active_game);

    // We need to create a temporary game to access the course
    let temp_game = PlatformRacer::new();
    let course = temp_game.course();

    // Render course tiles as colored cubes
    let solid_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.solid_tile),
        unlit: true,
        ..default()
    });
    let platform_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.grass_tile),
        unlit: true,
        ..default()
    });
    let hazard_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.hazard_tile),
        emissive: LinearRgba::new(2.0, 0.2, 0.1, 1.0),
        ..default()
    });
    let checkpoint_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.platform_tile),
        emissive: LinearRgba::new(0.2, 0.5, 2.0, 1.0),
        ..default()
    });
    let finish_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.platformer.finish_tile),
        emissive: LinearRgba::new(2.0, 1.7, 0.2, 1.0),
        ..default()
    });

    // Collect tile positions by material type for batched mesh construction.
    // This reduces ~800 individual draw calls down to ~5 (one per material).
    let mut solid_tiles: Vec<[f32; 3]> = Vec::new();
    let mut platform_tiles: Vec<[f32; 3]> = Vec::new();
    let mut hazard_tiles: Vec<[f32; 3]> = Vec::new();
    let mut checkpoint_tiles: Vec<[f32; 3]> = Vec::new();
    let mut finish_tiles: Vec<[f32; 3]> = Vec::new();

    for y in 0..course.height {
        for x in 0..course.width {
            let tile = course.get_tile(x as i32, y as i32);
            let wx = x as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            let wy = y as f32 * TILE_SIZE + TILE_SIZE / 2.0;
            let pos = [wx, wy, 0.0];
            match tile {
                Tile::Solid => solid_tiles.push(pos),
                Tile::Platform => platform_tiles.push(pos),
                Tile::Hazard => hazard_tiles.push(pos),
                Tile::Checkpoint => checkpoint_tiles.push(pos),
                Tile::Finish => finish_tiles.push(pos),
                _ => {},
            }
        }
    }

    let batches: [(&Vec<[f32; 3]>, &Handle<StandardMaterial>); 5] = [
        (&solid_tiles, &solid_mat),
        (&platform_tiles, &platform_mat),
        (&hazard_tiles, &hazard_mat),
        (&checkpoint_tiles, &checkpoint_mat),
        (&finish_tiles, &finish_mat),
    ];

    for (positions, mat) in &batches {
        if !positions.is_empty() {
            let mesh = build_batched_cuboid_mesh(positions, TILE_SIZE);
            commands.spawn((
                GameEntity,
                CourseTileEntity,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d((*mat).clone()),
            ));
        }
    }

    // Spawn player meshes
    let player_mesh = meshes.add(Cuboid::new(PLAYER_WIDTH, PLAYER_HEIGHT, PLAYER_WIDTH));
    let local_pid = network_role.local_player_id;
    for player in &lobby.players {
        if player.is_spectator {
            continue;
        }
        let color = player_color_to_bevy(&player.color);
        let alpha = if player.id == local_pid { 1.0 } else { 0.6 };
        let spawn_y = state
            .as_ref()
            .and_then(|s| s.players.get(&player.id))
            .map(|p| p.y)
            .unwrap_or(3.0);
        let spawn_x = state
            .as_ref()
            .and_then(|s| s.players.get(&player.id))
            .map(|p| p.x)
            .unwrap_or(2.0);

        commands.spawn((
            GameEntity,
            PlatformerPlayerEntity(player.id),
            Mesh3d(player_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(alpha),
                alpha_mode: if alpha < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            })),
            Transform::from_xyz(spawn_x, spawn_y, 0.0),
        ));
    }

    // HUD
    spawn_hud_text(
        &mut commands,
        PlatformerTimerText,
        "Time: 0.0s",
        18.0,
        Color::WHITE,
        HudPosition::TopLeft,
    );
    spawn_hud_text(
        &mut commands,
        PlatformerPositionText,
        "",
        18.0,
        Color::WHITE,
        HudPosition::TopRight,
    );

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &format!(
            "BREAKPOINT:SETUP_PLATFORMER course={}x{} local_pid={}",
            course.width, course.height, local_pid
        )
        .into(),
    );

    // Controls hint (bottom-left, auto-dismiss)
    commands.spawn((
        GameEntity,
        ControlsHint { timer: 8.0 },
        Text::new("A/D or Arrows to move\nSpace to jump\nE for power-up"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(rgba(&theme.platformer.hud_text)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

fn platformer_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut local_input: ResMut<PlatformerLocalInput>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    if network_role.is_spectator {
        return;
    }

    let mut move_dir = 0.0;
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        move_dir -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        move_dir += 1.0;
    }
    local_input.move_dir = move_dir;
    local_input.jump =
        keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::ArrowUp);

    if local_input.jump {
        audio_queue.push(crate::audio::AudioEvent::PlatformerJump);
    }
    if keyboard.just_pressed(KeyCode::KeyE) {
        audio_queue.push(crate::audio::AudioEvent::PlatformerPowerUp);
    }

    // Build and send/apply input
    let input = PlatformerInput {
        move_dir: local_input.move_dir,
        jump: local_input.jump,
        use_powerup: keyboard.just_pressed(KeyCode::KeyE),
    };

    send_player_input(&input, &mut active_game, &network_role, &ws_client);
}

fn platformer_render_sync(
    active_game: Res<ActiveGame>,
    time: Res<Time>,
    mut player_query: Query<(&PlatformerPlayerEntity, &mut Transform, &mut Visibility)>,
) {
    let state: Option<PlatformerState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    let lerp_factor = (15.0 * time.delta_secs()).min(1.0);
    for (entity, mut transform, mut visibility) in &mut player_query {
        if let Some(ps) = state.players.get(&entity.0) {
            if ps.eliminated {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                let target = Vec3::new(ps.x, ps.y, 0.0);
                transform.translation = transform.translation.lerp(target, lerp_factor);
            }
        }
    }
}

fn platformer_hud_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut timer_text: Query<&mut Text, With<PlatformerTimerText>>,
    mut pos_text: Query<&mut Text, (With<PlatformerPositionText>, Without<PlatformerTimerText>)>,
) {
    let state: Option<PlatformerState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    if let Ok(mut text) = timer_text.single_mut() {
        **text = format!("Time: {:.1}s", state.round_timer);
    }

    if let Ok(mut text) = pos_text.single_mut()
        && let Some(player) = state.players.get(&network_role.local_player_id)
    {
        if player.finished {
            let pos = state
                .finish_order
                .iter()
                .position(|&id| id == network_role.local_player_id)
                .map(|i| i + 1)
                .unwrap_or(0);
            **text = format!("Finished #{pos}!");
        } else {
            **text = format!("X: {:.0}", player.x);
        }
    }
}

/// Build a single merged mesh from multiple cuboid positions.
/// Each cuboid is a full 6-face box centered at the given position.
fn build_batched_cuboid_mesh(positions: &[[f32; 3]], size: f32) -> Mesh {
    let half = size / 2.0;
    let cap_v = positions.len() * 24;
    let cap_i = positions.len() * 36;
    let mut verts: Vec<[f32; 3]> = Vec::with_capacity(cap_v);
    let mut norms: Vec<[f32; 3]> = Vec::with_capacity(cap_v);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(cap_v);
    let mut idxs: Vec<u32> = Vec::with_capacity(cap_i);

    for &[cx, cy, cz] in positions {
        // Each face: 4 vertices + 6 indices (2 triangles)
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            // +Z
            (
                [0.0, 0.0, 1.0],
                [
                    [cx - half, cy - half, cz + half],
                    [cx + half, cy - half, cz + half],
                    [cx + half, cy + half, cz + half],
                    [cx - half, cy + half, cz + half],
                ],
            ),
            // -Z
            (
                [0.0, 0.0, -1.0],
                [
                    [cx + half, cy - half, cz - half],
                    [cx - half, cy - half, cz - half],
                    [cx - half, cy + half, cz - half],
                    [cx + half, cy + half, cz - half],
                ],
            ),
            // +Y
            (
                [0.0, 1.0, 0.0],
                [
                    [cx - half, cy + half, cz + half],
                    [cx + half, cy + half, cz + half],
                    [cx + half, cy + half, cz - half],
                    [cx - half, cy + half, cz - half],
                ],
            ),
            // -Y
            (
                [0.0, -1.0, 0.0],
                [
                    [cx - half, cy - half, cz - half],
                    [cx + half, cy - half, cz - half],
                    [cx + half, cy - half, cz + half],
                    [cx - half, cy - half, cz + half],
                ],
            ),
            // +X
            (
                [1.0, 0.0, 0.0],
                [
                    [cx + half, cy - half, cz + half],
                    [cx + half, cy - half, cz - half],
                    [cx + half, cy + half, cz - half],
                    [cx + half, cy + half, cz + half],
                ],
            ),
            // -X
            (
                [-1.0, 0.0, 0.0],
                [
                    [cx - half, cy - half, cz - half],
                    [cx - half, cy - half, cz + half],
                    [cx - half, cy + half, cz + half],
                    [cx - half, cy + half, cz - half],
                ],
            ),
        ];

        for (normal, face_verts) in &faces {
            let b = verts.len() as u32;
            verts.extend_from_slice(face_verts);
            norms.extend_from_slice(&[*normal; 4]);
            uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            idxs.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, verts);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, norms);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(idxs));
    mesh
}

fn cleanup_platformer(mut commands: Commands) {
    commands.remove_resource::<PlatformerLocalInput>();
}
