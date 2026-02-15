use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::{GameId, PlayerId};
use breakpoint_lasertag::arena::WallType;
use breakpoint_lasertag::projectile::PLAYER_RADIUS;
use breakpoint_lasertag::{LaserTagArena, LaserTagInput, LaserTagState};

use crate::app::AppState;
use crate::net_client::WsClient;
use crate::shaders::glow_material::GlowMaterial;
use crate::theme::{Theme, rgb, rgba};

use super::{
    ActiveGame, ControlsHint, GameEntity, GameRegistry, HudPosition, NetworkRole, cursor_to_ground,
    player_color_to_bevy, read_game_state, send_player_input, spawn_hud_text,
};

pub struct LaserTagPlugin;

impl Plugin for LaserTagPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, register_lasertag)
            .add_systems(
                Update,
                (
                    setup_lasertag.run_if(lasertag_needs_setup),
                    ApplyDeferred,
                    (
                        lasertag_input_system,
                        lasertag_render_sync,
                        laser_trail_render_system,
                        lasertag_hud_system,
                    ),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame).and(is_lasertag_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_lasertag.run_if(resource_exists::<LaserTagLocalInput>),
            );
    }
}

fn register_lasertag(mut registry: ResMut<GameRegistry>) {
    registry.register(GameId::LaserTag, || Box::new(LaserTagArena::new()));
}

fn is_lasertag_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == GameId::LaserTag)
}

fn lasertag_needs_setup(input: Option<Res<LaserTagLocalInput>>) -> bool {
    input.is_none()
}

/// Local input state for laser tag.
#[derive(Resource, Default)]
struct LaserTagLocalInput {
    aim_angle: f32,
}

/// Marker for player mesh entities.
#[derive(Component)]
struct LaserTagPlayerEntity(PlayerId);

/// Marker for aim direction indicator.
#[derive(Component)]
struct AimIndicator(PlayerId);

/// Marker for HUD score text.
#[derive(Component)]
struct LaserTagScoreText;

/// Marker for HUD timer text.
#[derive(Component)]
struct LaserTagTimerText;

/// Marker for laser trail pool entities.
#[derive(Component)]
struct LaserTrailEntity;

/// Index into the pre-allocated laser trail pool.
#[derive(Component)]
struct TrailPoolSlot(usize);

/// Pre-spawned pool of laser trail entities to avoid per-frame entity churn.
#[derive(Resource)]
struct LaserTrailPool {
    materials: Vec<Handle<GlowMaterial>>,
}

const MAX_TRAIL_SEGMENTS: usize = 64;

#[allow(clippy::too_many_arguments)]
fn setup_lasertag(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut glow_materials: ResMut<Assets<GlowMaterial>>,
    lobby: Res<crate::lobby::LobbyState>,
    network_role: Res<NetworkRole>,
    active_game: Res<ActiveGame>,
    theme: Res<Theme>,
) {
    commands.insert_resource(LaserTagLocalInput::default());

    // Access arena from a temp game (for geometry setup)
    let temp_game = LaserTagArena::new();
    let arena = temp_game.arena();

    // Arena floor (dark plane on XZ)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::new(arena.width / 2.0, arena.depth / 2.0),
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: rgb(&theme.lasertag.arena_floor),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(arena.width / 2.0, 0.0, arena.depth / 2.0),
    ));

    // Render walls
    let solid_wall_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.lasertag.wall_solid),
        unlit: true,
        ..default()
    });
    let reflective_wall_mat = materials.add(StandardMaterial {
        base_color: rgb(&theme.lasertag.wall_reflective),
        emissive: LinearRgba::new(0.5, 0.7, 1.5, 1.0),
        ..default()
    });

    for wall in &arena.walls {
        let dx = wall.bx - wall.ax;
        let dz = wall.bz - wall.az;
        let length = (dx * dx + dz * dz).sqrt();
        let cx = (wall.ax + wall.bx) / 2.0;
        let cz = (wall.az + wall.bz) / 2.0;
        let angle = dz.atan2(dx);
        let wall_height = 2.0;

        let mat = match wall.wall_type {
            WallType::Solid => solid_wall_mat.clone(),
            WallType::Reflective => reflective_wall_mat.clone(),
        };

        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Cuboid::new(length, wall_height, 0.3))),
            MeshMaterial3d(mat),
            Transform::from_xyz(cx, wall_height / 2.0, cz)
                .with_rotation(Quat::from_rotation_y(-angle)),
        ));
    }

    // Smoke zones as semi-transparent circles
    let smoke_mat = materials.add(StandardMaterial {
        base_color: rgba(&theme.lasertag.smoke_zone),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    for &(sx, sz, radius) in &arena.smoke_zones {
        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Cylinder::new(radius, 0.05))),
            MeshMaterial3d(smoke_mat.clone()),
            Transform::from_xyz(sx, 0.02, sz),
        ));
    }

    // Spawn player meshes (circles on XZ plane)
    let player_mesh = meshes.add(Cylinder::new(PLAYER_RADIUS, 1.5));
    let local_pid = network_role.local_player_id;

    // Get current state for initial positions
    let state: Option<LaserTagState> = read_game_state(&active_game);

    let mut _spawned_count = 0u32;
    for player in &lobby.players {
        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "BREAKPOINT:LT_PLAYER id={} spectator={} color=({},{},{})",
                player.id, player.is_spectator, player.color.r, player.color.g, player.color.b
            )
            .into(),
        );
        if player.is_spectator {
            continue;
        }
        let color = player_color_to_bevy(&player.color);
        let alpha = if player.id == local_pid { 1.0 } else { 0.7 };

        let (px, pz) = state
            .as_ref()
            .and_then(|s| s.players.get(&player.id))
            .map(|p| (p.x, p.z))
            .unwrap_or((arena.width / 2.0, arena.depth / 2.0));

        #[cfg(target_arch = "wasm32")]
        web_sys::console::log_1(
            &format!(
                "BREAKPOINT:LT_SPAWN id={} pos=({px:.1}, {pz:.1}) alpha={alpha}",
                player.id
            )
            .into(),
        );

        commands.spawn((
            GameEntity,
            LaserTagPlayerEntity(player.id),
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
            Transform::from_xyz(px, 0.75, pz),
        ));
        _spawned_count += 1;

        // Aim direction line
        commands.spawn((
            GameEntity,
            AimIndicator(player.id),
            Mesh3d(meshes.add(Cuboid::new(2.0, 0.05, 0.05))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(0.5),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(px, 0.5, pz),
        ));
    }

    // Pre-spawn laser trail entity pool (avoids per-frame entity churn)
    let trail_mesh = meshes.add(Cuboid::new(1.0, 0.08, 0.06));
    let mut trail_pool_materials = Vec::with_capacity(MAX_TRAIL_SEGMENTS);
    for i in 0..MAX_TRAIL_SEGMENTS {
        let mat = glow_materials.add(GlowMaterial::new(
            LinearRgba::new(0.3, 0.9, 2.0, 1.0),
            1.5,
            0.0,
        ));
        trail_pool_materials.push(mat.clone());
        commands.spawn((
            GameEntity,
            LaserTrailEntity,
            TrailPoolSlot(i),
            Mesh3d(trail_mesh.clone()),
            MeshMaterial3d(mat),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(LaserTrailPool {
        materials: trail_pool_materials,
    });

    // HUD
    spawn_hud_text(
        &mut commands,
        LaserTagScoreText,
        "Score: 0",
        20.0,
        Color::WHITE,
        HudPosition::TopRight,
    );
    spawn_hud_text(
        &mut commands,
        LaserTagTimerText,
        "Time: 0s",
        18.0,
        Color::WHITE,
        HudPosition::TopLeft,
    );

    #[cfg(target_arch = "wasm32")]
    web_sys::console::log_1(
        &format!(
            "BREAKPOINT:SETUP_LASERTAG arena={}x{} local_pid={} spawned={_spawned_count}",
            arena.width, arena.depth, local_pid
        )
        .into(),
    );

    // Controls hint (bottom-left, auto-dismiss)
    commands.spawn((
        GameEntity,
        ControlsHint { timer: 8.0 },
        Text::new("WASD to move\nMouse to aim\nClick to fire\nE for power-up"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(rgba(&theme.lasertag.hud_text)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn lasertag_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<
        &Transform,
        (
            With<crate::camera::GameCamera>,
            Without<crate::camera::GameLight>,
        ),
    >,
    mut local_input: ResMut<LaserTagLocalInput>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    if network_role.is_spectator {
        return;
    }

    // Movement (WASD)
    let mut move_x: f32 = 0.0;
    let mut move_z: f32 = 0.0;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        move_z += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        move_z -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        move_x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        move_x += 1.0;
    }

    // Normalize diagonal movement
    let len = (move_x * move_x + move_z * move_z).sqrt();
    if len > 1.0 {
        move_x /= len;
        move_z /= len;
    }

    // Aim: manual ray-ground intersection (same pattern as golf).
    // Avoids Camera::viewport_to_world() which silently returns Err in
    // WASM/WebGL2 when Camera.computed is unpopulated or stale.
    if let Ok(window) = windows.single()
        && let Some(cursor_pos) = window.cursor_position()
        && let Ok(cam_transform) = cameras.single()
        && let Some(ground_point) = cursor_to_ground(cursor_pos, window, cam_transform)
    {
        // Get player position from state
        let state: Option<LaserTagState> = read_game_state(&active_game);
        if let Some(ps) = state.and_then(|s| s.players.get(&network_role.local_player_id).cloned())
        {
            let dx = ground_point.x - ps.x;
            let dz = ground_point.z - ps.z;
            local_input.aim_angle = dz.atan2(dx);
        }
    }

    let fire = mouse.pressed(MouseButton::Left);

    if mouse.just_pressed(MouseButton::Left) {
        audio_queue.push(crate::audio::AudioEvent::LaserFire);
    }

    let input = LaserTagInput {
        move_x,
        move_z,
        aim_angle: local_input.aim_angle,
        fire,
        use_powerup: keyboard.just_pressed(KeyCode::KeyE),
    };

    send_player_input(&input, &mut active_game, &network_role, &ws_client);
}

fn lasertag_render_sync(
    active_game: Res<ActiveGame>,
    time: Res<Time>,
    mut player_query: Query<
        (
            &LaserTagPlayerEntity,
            &mut Transform,
            &mut Visibility,
            &MeshMaterial3d<StandardMaterial>,
        ),
        Without<AimIndicator>,
    >,
    mut aim_query: Query<(&AimIndicator, &mut Transform), Without<LaserTagPlayerEntity>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let state: Option<LaserTagState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    // DEBUG: log render sync details for first few ticks
    #[cfg(target_arch = "wasm32")]
    if active_game.tick <= 3 {
        let entity_count = player_query.iter().count();
        let state_count = state.players.len();
        let positions: Vec<String> = state
            .players
            .iter()
            .map(|(id, ps)| format!("p{}=({:.1},{:.1})", id, ps.x, ps.z))
            .collect();
        web_sys::console::log_1(
            &format!(
                "BREAKPOINT:LT_RENDER_SYNC tick={} entities={entity_count} \
                 state_players={state_count} positions=[{}]",
                active_game.tick,
                positions.join(", ")
            )
            .into(),
        );
    }

    let lerp_factor = (15.0 * time.delta_secs()).min(1.0);
    for (entity, mut transform, mut visibility, mat_handle) in &mut player_query {
        if let Some(ps) = state.players.get(&entity.0) {
            *visibility = Visibility::Visible;
            let target = Vec3::new(ps.x, transform.translation.y, ps.z);
            transform.translation = transform.translation.lerp(target, lerp_factor);

            // Stun visual feedback: reduce alpha when stunned
            if let Some(mat) = materials.get_mut(mat_handle) {
                let current_alpha = mat.base_color.alpha();
                if ps.is_stunned() {
                    if current_alpha > 0.4 {
                        mat.base_color.set_alpha(0.4);
                        mat.alpha_mode = AlphaMode::Blend;
                    }
                } else if current_alpha < 0.7 {
                    // Restore normal alpha (0.7 for remote, 1.0 for local — use 0.7 as safe floor)
                    mat.base_color.set_alpha(current_alpha.max(0.7));
                }
            }
        }
    }

    // Update aim indicators
    for (aim, mut transform) in &mut aim_query {
        if let Some(ps) = state.players.get(&aim.0) {
            let aim_len = 1.0;
            let offset_x = ps.aim_angle.cos() * aim_len;
            let offset_z = ps.aim_angle.sin() * aim_len;
            transform.translation = Vec3::new(ps.x + offset_x, 0.5, ps.z + offset_z);
            transform.rotation = Quat::from_rotation_y(-ps.aim_angle);
        }
    }
}

fn lasertag_hud_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut score_text: Query<&mut Text, With<LaserTagScoreText>>,
    mut timer_text: Query<&mut Text, (With<LaserTagTimerText>, Without<LaserTagScoreText>)>,
) {
    let state: Option<LaserTagState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };

    if let Ok(mut text) = score_text.single_mut() {
        let tags = state
            .tags_scored
            .get(&network_role.local_player_id)
            .copied()
            .unwrap_or(0);
        **text = format!("Tags: {tags}");
    }

    if let Ok(mut text) = timer_text.single_mut() {
        let remaining = (180.0 - state.round_timer).max(0.0);
        **text = format!("Time: {:.0}s", remaining);
    }
}

/// Render laser trail segments by updating pre-spawned pool entities.
/// Uses transform scaling + material alpha updates instead of despawn/respawn.
fn laser_trail_render_system(
    active_game: Res<ActiveGame>,
    pool: Option<Res<LaserTrailPool>>,
    mut trail_query: Query<
        (&TrailPoolSlot, &mut Transform, &mut Visibility),
        With<LaserTrailEntity>,
    >,
    mut glow_materials: ResMut<Assets<GlowMaterial>>,
) {
    let Some(pool) = pool else {
        return;
    };

    let state: Option<LaserTagState> = read_game_state(&active_game);

    // Collect active segments
    let mut segments: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
    if let Some(state) = &state {
        for trail in &state.laser_trails {
            let alpha = (1.0 - trail.age / 0.3).clamp(0.0, 1.0);
            if alpha < 0.01 {
                continue;
            }
            for &(sx, sz, ex, ez) in &trail.segments {
                let dx = ex - sx;
                let dz = ez - sz;
                let length = (dx * dx + dz * dz).sqrt();
                if length < 0.01 {
                    continue;
                }
                segments.push((sx, sz, ex, ez, alpha));
                if segments.len() >= MAX_TRAIL_SEGMENTS {
                    break;
                }
            }
            if segments.len() >= MAX_TRAIL_SEGMENTS {
                break;
            }
        }
    }

    // Update pool entities: show active segments, hide the rest
    for (slot, mut transform, mut visibility) in &mut trail_query {
        if slot.0 < segments.len() {
            let (sx, sz, ex, ez, alpha) = segments[slot.0];
            let dx = ex - sx;
            let dz = ez - sz;
            let length = (dx * dx + dz * dz).sqrt();
            let cx = (sx + ex) / 2.0;
            let cz = (sz + ez) / 2.0;
            let angle = dz.atan2(dx);

            *transform = Transform::from_xyz(cx, 1.0, cz)
                .with_rotation(Quat::from_rotation_y(-angle))
                .with_scale(Vec3::new(length, 1.0, 1.0));
            *visibility = Visibility::Visible;

            if let Some(mat) = glow_materials.get_mut(&pool.materials[slot.0]) {
                mat.params.y = alpha;
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn cleanup_lasertag(mut commands: Commands) {
    commands.remove_resource::<LaserTagLocalInput>();
    commands.remove_resource::<LaserTrailPool>();
}
