use std::collections::HashSet;

use bevy::ecs::system::NonSend;
use bevy::prelude::*;

use breakpoint_core::game_trait::PlayerId;
use breakpoint_golf::course::all_courses;
use breakpoint_golf::physics::{BALL_RADIUS, HOLE_RADIUS};
use breakpoint_golf::{GolfInput, GolfState, MiniGolf};

use crate::app::AppState;
use crate::camera::GameCamera;
use crate::net_client::WsClient;

use super::{
    ActiveGame, ControlsHint, GameEntity, GameRegistry, HudPosition, NetworkRole,
    player_color_to_bevy, read_game_state, send_player_input, spawn_hud_text,
};

pub struct GolfPlugin;

impl Plugin for GolfPlugin {
    fn build(&self, app: &mut App) {
        // setup_golf is run during Update (not OnEnter) because setup_game
        // inserts ActiveGame via deferred commands on OnEnter(InGame). Those
        // commands aren't flushed until after the OnEnter schedule completes,
        // so a run_if(is_golf_active) check on OnEnter would always fail.
        // By running in Update with an explicit apply_deferred chain, we
        // guarantee resources are available to subsequent golf systems.
        app.add_systems(Startup, register_golf)
            .add_systems(
                Update,
                (
                    setup_golf.run_if(golf_needs_setup),
                    ApplyDeferred,
                    (
                        golf_input_system,
                        golf_apply_input_system,
                        golf_render_sync,
                        aim_line_system,
                        power_bar_system,
                        stroke_counter_system,
                        hole_info_system,
                        scoreboard_system,
                        sink_flash_system,
                    ),
                )
                    .chain()
                    .run_if(in_state(AppState::InGame).and(is_golf_active)),
            )
            .add_systems(
                OnExit(AppState::InGame),
                cleanup_golf.run_if(resource_exists::<GolfLocalInput>),
            );
    }
}

fn register_golf(mut registry: ResMut<GameRegistry>) {
    registry.register("mini-golf", || Box::new(MiniGolf::new()));
}

fn is_golf_active(game: Option<Res<ActiveGame>>) -> bool {
    game.is_some_and(|g| g.game_id == "mini-golf")
}

fn golf_needs_setup(input: Option<Res<GolfLocalInput>>) -> bool {
    input.is_none()
}

/// Course metadata exposed to other systems (camera, between-rounds UI).
#[derive(Resource, Clone)]
pub struct GolfCourseInfo {
    pub hole_index: usize,
    pub hole_name: String,
    pub par: u8,
    pub total_holes: usize,
    pub width: f32,
    pub depth: f32,
}

/// Current local input state for golf (built from mouse/keyboard).
#[derive(Resource, Default)]
struct GolfLocalInput {
    aim_angle: f32,
    power: f32,
    stroke_requested: bool,
}

/// Tracks which players have been seen as sunk (for sink flash detection).
#[derive(Resource, Default)]
struct SunkTracker {
    seen_sunk: HashSet<PlayerId>,
}

/// Marker for ball mesh entities, keyed by player id.
#[derive(Component)]
struct BallEntity(PlayerId);

/// Marker for aim line dot meshes.
#[derive(Component)]
struct AimDot(u8);

/// Marker for the power bar fill.
#[derive(Component)]
struct PowerBarFill;

/// Marker for the power bar label.
#[derive(Component)]
struct PowerBarLabel;

/// Marker for the stroke counter text.
#[derive(Component)]
struct StrokeCounterText;

/// Marker for the hole info header text.
#[derive(Component)]
struct HoleInfoText;

/// Marker for the mini-scoreboard text.
#[derive(Component)]
struct ScoreboardText;

/// Marker for the ground disc under the local player's ball.
#[derive(Component)]
struct BallMarker;

/// Marker for sink flash entities.
#[derive(Component)]
struct SinkFlash {
    timer: f32,
}

#[allow(clippy::too_many_arguments)]
fn setup_golf(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    lobby: Res<crate::lobby::LobbyState>,
    network_role: Res<NetworkRole>,
    active_game: Res<ActiveGame>,
) {
    commands.insert_resource(GolfLocalInput::default());
    commands.insert_resource(SunkTracker::default());

    // Get course info from the active game's serialized state
    let state: Option<GolfState> = read_game_state(&active_game);
    let course_index = state.map(|s| s.course_index as usize).unwrap_or(0);

    let courses = all_courses();
    let course_data = &courses[course_index.min(courses.len() - 1)];

    commands.insert_resource(GolfCourseInfo {
        hole_index: course_index,
        hole_name: course_data.name.clone(),
        par: course_data.par,
        total_holes: courses.len(),
        width: course_data.width,
        depth: course_data.depth,
    });

    // --- Environment ---

    // Large dark-green ground plane beneath the course
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(Vec3::Y, Vec2::new(50.0, 50.0)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.08, 0.35, 0.08),
            perceptual_roughness: 1.0,
            ..default()
        })),
        Transform::from_xyz(course_data.width / 2.0, -0.01, course_data.depth / 2.0),
    ));

    // Course floor (grass)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(
            Vec3::Y,
            Vec2::new(course_data.width / 2.0, course_data.depth / 2.0),
        ))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.65, 0.18),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(course_data.width / 2.0, 0.0, course_data.depth / 2.0),
    ));

    // Course border (raised lip)
    let border_thickness = 0.15;
    let border_height = 0.12;
    let border_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.2, 0.1),
        perceptual_roughness: 0.8,
        ..default()
    });
    // Bottom border
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cuboid::new(
            course_data.width + border_thickness * 2.0,
            border_height,
            border_thickness,
        ))),
        MeshMaterial3d(border_mat.clone()),
        Transform::from_xyz(
            course_data.width / 2.0,
            border_height / 2.0,
            -border_thickness / 2.0,
        ),
    ));
    // Top border
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cuboid::new(
            course_data.width + border_thickness * 2.0,
            border_height,
            border_thickness,
        ))),
        MeshMaterial3d(border_mat.clone()),
        Transform::from_xyz(
            course_data.width / 2.0,
            border_height / 2.0,
            course_data.depth + border_thickness / 2.0,
        ),
    ));
    // Left border
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cuboid::new(
            border_thickness,
            border_height,
            course_data.depth,
        ))),
        MeshMaterial3d(border_mat.clone()),
        Transform::from_xyz(
            -border_thickness / 2.0,
            border_height / 2.0,
            course_data.depth / 2.0,
        ),
    ));
    // Right border
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cuboid::new(
            border_thickness,
            border_height,
            course_data.depth,
        ))),
        MeshMaterial3d(border_mat.clone()),
        Transform::from_xyz(
            course_data.width + border_thickness / 2.0,
            border_height / 2.0,
            course_data.depth / 2.0,
        ),
    ));

    // Walls (wood tone)
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.2, 0.1),
        perceptual_roughness: 0.85,
        ..default()
    });
    for wall in &course_data.walls {
        let dx = wall.b.x - wall.a.x;
        let dz = wall.b.z - wall.a.z;
        let length = (dx * dx + dz * dz).sqrt();
        let cx = (wall.a.x + wall.b.x) / 2.0;
        let cz = (wall.a.z + wall.b.z) / 2.0;
        let angle = dz.atan2(dx);
        let thickness = 0.3;

        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Cuboid::new(length, wall.height, thickness))),
            MeshMaterial3d(wall_mat.clone()),
            Transform::from_xyz(cx, wall.height / 2.0, cz)
                .with_rotation(Quat::from_rotation_y(-angle)),
        ));
    }

    // Bumpers (metallic silver-blue, distinct from player ball colors)
    let bumper_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.55, 0.65),
        metallic: 0.9,
        perceptual_roughness: 0.2,
        ..default()
    });
    for bumper in &course_data.bumpers {
        commands.spawn((
            GameEntity,
            Mesh3d(meshes.add(Sphere::new(bumper.radius))),
            MeshMaterial3d(bumper_mat.clone()),
            Transform::from_xyz(bumper.position.x, bumper.radius, bumper.position.z),
        ));
    }

    // Hole (dark cylinder + flag)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cylinder::new(HOLE_RADIUS, 0.05))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.03, 0.03, 0.03),
            ..default()
        })),
        Transform::from_xyz(
            course_data.hole_position.x,
            0.01,
            course_data.hole_position.z,
        ),
    ));

    // Flag pole (thin cylinder)
    let pole_height = 2.5;
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Cylinder::new(0.04, pole_height))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.7, 0.7, 0.7),
            metallic: 0.6,
            ..default()
        })),
        Transform::from_xyz(
            course_data.hole_position.x,
            pole_height / 2.0,
            course_data.hole_position.z,
        ),
    ));

    // Flag (small colored plane at top of pole)
    commands.spawn((
        GameEntity,
        Mesh3d(meshes.add(Plane3d::new(Vec3::X, Vec2::new(0.3, 0.2)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.2, 0.2),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(
            course_data.hole_position.x,
            pole_height - 0.2,
            course_data.hole_position.z + 0.3,
        ),
    ));

    // Ball meshes for each player
    let ball_mesh = meshes.add(Sphere::new(BALL_RADIUS));
    let local_player_id = network_role.local_player_id;
    for player in &lobby.players {
        if player.is_spectator {
            continue;
        }
        let color = player_color_to_bevy(&player.color);
        let is_local = player.id == local_player_id;
        let alpha = if is_local { 1.0 } else { 0.6 };
        commands.spawn((
            GameEntity,
            BallEntity(player.id),
            Mesh3d(ball_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color.with_alpha(alpha),
                // Local player ball glows slightly to stand out from obstacles
                emissive: if is_local {
                    color.to_linear() * 0.4
                } else {
                    LinearRgba::NONE
                },
                alpha_mode: if alpha < 1.0 {
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                metallic: 0.3,
                perceptual_roughness: 0.5,
                ..default()
            })),
            Transform::from_xyz(
                course_data.spawn_point.x,
                BALL_RADIUS,
                course_data.spawn_point.z,
            ),
        ));
    }

    // Ground marker disc under local player's ball for visibility
    commands.spawn((
        GameEntity,
        BallMarker,
        Mesh3d(meshes.add(Cylinder::new(0.6, 0.01))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 1.0, 1.0, 0.5),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(course_data.spawn_point.x, 0.01, course_data.spawn_point.z),
    ));

    // Aim dots (5 small spheres along aim direction)
    let dot_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 1.0, 0.3),
        unlit: true,
        ..default()
    });
    let dot_mesh = meshes.add(Sphere::new(0.08));
    for i in 0..5u8 {
        commands.spawn((
            GameEntity,
            AimDot(i),
            Mesh3d(dot_mesh.clone()),
            MeshMaterial3d(dot_mat.clone()),
            Transform::from_xyz(0.0, 0.15, 0.0),
            Visibility::Hidden,
        ));
    }

    // --- UI ---

    // Hole info header (top-left)
    spawn_hud_text(
        &mut commands,
        HoleInfoText,
        format!(
            "Hole {} of {} — {} — Par {}",
            course_index + 1,
            courses.len(),
            course_data.name,
            course_data.par
        ),
        18.0,
        Color::srgb(0.9, 0.9, 0.9),
        HudPosition::TopLeft,
    );

    // Power bar with gradient fill and label
    commands
        .spawn((
            GameEntity,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(20.0),
                left: Val::Percent(35.0),
                width: Val::Percent(30.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: Val::Px(2.0),
                ..default()
            },
        ))
        .with_children(|parent| {
            parent.spawn((
                PowerBarLabel,
                Text::new("POWER"),
                TextFont {
                    font_size: 12.0,
                    ..default()
                },
                TextColor(Color::srgb(0.8, 0.8, 0.8)),
            ));
            parent
                .spawn((
                    Node {
                        width: Val::Percent(100.0),
                        height: Val::Px(20.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.15, 0.15, 0.15, 0.8)),
                ))
                .with_children(|bar| {
                    bar.spawn((
                        PowerBarFill,
                        Node {
                            width: Val::Percent(0.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.2, 0.8, 0.2)),
                    ));
                });
        });

    // Stroke counter (top-right)
    spawn_hud_text(
        &mut commands,
        StrokeCounterText,
        "Strokes: 0",
        18.0,
        Color::WHITE,
        HudPosition::TopRight,
    );

    // Mini-scoreboard (right side)
    commands.spawn((
        GameEntity,
        ScoreboardText,
        Text::new(""),
        TextFont {
            font_size: 14.0,
            ..default()
        },
        TextColor(Color::srgb(0.85, 0.85, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(35.0),
            right: Val::Px(10.0),
            ..default()
        },
    ));

    // Controls hint (bottom-left, auto-dismiss)
    commands.spawn((
        GameEntity,
        ControlsHint { timer: 8.0 },
        Text::new("Hold LMB to charge\nRelease to stroke\nMove mouse to aim"),
        TextFont {
            font_size: 16.0,
            ..default()
        },
        TextColor(Color::srgba(0.9, 0.9, 0.9, 0.85)),
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(60.0),
            left: Val::Px(10.0),
            ..default()
        },
    ));
}

/// Gather mouse input and populate GolfLocalInput (no network or game mutation).
fn golf_input_system(
    windows: Query<&Window>,
    cameras: Query<&Transform, With<GameCamera>>,
    mut local_input: ResMut<GolfLocalInput>,
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    if network_role.is_spectator {
        return;
    }

    // Power charging works regardless of cursor position — prevents losing
    // charge if the cursor briefly leaves the canvas.
    if mouse.pressed(MouseButton::Left) {
        local_input.power = (local_input.power + 0.025).min(1.0);
    }
    if mouse.just_released(MouseButton::Left) && local_input.power > 0.01 {
        local_input.power = local_input.power.max(0.15); // Minimum visible stroke
        local_input.stroke_requested = true;
    }
    if !mouse.pressed(MouseButton::Left) && !local_input.stroke_requested {
        local_input.power = 0.0;
    }

    // Aim angle needs cursor position + camera for raycasting.
    // If unavailable, aim_angle retains its previous value.
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        return;
    };
    let Ok(cam_transform) = cameras.single() else {
        return;
    };

    let state: Option<GolfState> = read_game_state(&active_game);
    let ball_pos = state.as_ref().and_then(|s| {
        s.balls
            .get(&network_role.local_player_id)
            .map(|b| Vec3::new(b.position.x, BALL_RADIUS, b.position.z))
    });

    let Some(ball_pos) = ball_pos else {
        return;
    };

    // Manual ray-ground intersection — bypasses Camera.computed which can
    // be unpopulated or stale in WASM/WebGL2, causing viewport_to_world
    // to silently return Err every frame.
    if let Some(ground_point) = cursor_to_ground(cursor_pos, window, cam_transform) {
        let dx = ground_point.x - ball_pos.x;
        let dz = ground_point.z - ball_pos.z;
        local_input.aim_angle = dz.atan2(dx);
    }
}

/// Project a screen-space cursor position onto the Y=0 ground plane using
/// the camera's current Transform (no dependency on Camera.computed).
fn cursor_to_ground(cursor_pos: Vec2, window: &Window, cam: &Transform) -> Option<Vec3> {
    let w = window.width();
    let h = window.height();
    if w < 1.0 || h < 1.0 {
        return None;
    }

    // Cursor to NDC: x ∈ [-1,1] (left to right), y ∈ [-1,1] (bottom to top)
    let ndc_x = (cursor_pos.x / w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (cursor_pos.y / h) * 2.0;

    // Camera3d default vertical FOV = π/4 (45°)
    let half_v = (std::f32::consts::FRAC_PI_4 * 0.5).tan();
    let half_h = half_v * (w / h);

    // Build world-space ray direction from camera axes.
    // Bevy's looking_at rotation places the local +X axis opposite to
    // screen-right in world space, so negate it to get the correct
    // screen-right direction for ray construction.
    let forward = *cam.forward();
    let right = -*cam.right();
    let up = *cam.up();
    let ray_dir = (forward + right * (ndc_x * half_h) + up * (ndc_y * half_v)).normalize();

    // Intersect with Y=0 plane
    if ray_dir.y.abs() < 1e-6 {
        return None;
    }
    let t = -cam.translation.y / ray_dir.y;
    if t <= 0.0 {
        return None;
    }
    Some(cam.translation + ray_dir * t)
}

/// Apply golf input: host applies directly, non-host sends via WS.
/// Only fires when the local player's ball is stopped — prevents misleading
/// audio feedback and wasted network messages while the ball is in motion.
fn golf_apply_input_system(
    mut local_input: ResMut<GolfLocalInput>,
    mut active_game: ResMut<ActiveGame>,
    network_role: Res<NetworkRole>,
    ws_client: NonSend<WsClient>,
    mut audio_queue: ResMut<crate::audio::AudioEventQueue>,
) {
    if !local_input.stroke_requested || network_role.is_spectator {
        return;
    }

    // Check if ball is actually ready for a stroke before sending input.
    // The game engine also checks this, but checking here lets us gate audio
    // feedback accurately — no sound when the ball is still rolling.
    let state: Option<GolfState> = read_game_state(&active_game);
    let can_stroke = state.as_ref().is_some_and(|s| {
        s.balls
            .get(&network_role.local_player_id)
            .is_some_and(|b| b.is_stopped() && !b.is_sunk)
    });

    if !can_stroke {
        local_input.stroke_requested = false;
        local_input.power = 0.0;
        return;
    }

    let input = GolfInput {
        aim_angle: local_input.aim_angle,
        power: local_input.power,
        stroke: true,
    };

    audio_queue.push(crate::audio::AudioEvent::GolfStroke);

    send_player_input(&input, &mut active_game, &network_role, &ws_client);

    local_input.stroke_requested = false;
    local_input.power = 0.0;
}

#[allow(clippy::type_complexity)]
fn golf_render_sync(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut ball_query: Query<(&BallEntity, &mut Transform, &mut Visibility), Without<BallMarker>>,
    mut marker_query: Query<
        (&mut Transform, &mut Visibility),
        (With<BallMarker>, Without<BallEntity>),
    >,
) {
    let state: Option<GolfState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };
    for (ball_entity, mut transform, mut visibility) in &mut ball_query {
        if let Some(ball) = state.balls.get(&ball_entity.0) {
            if ball.is_sunk {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                transform.translation.x = ball.position.x;
                transform.translation.y = BALL_RADIUS;
                transform.translation.z = ball.position.z;
            }
        }
    }

    // Sync ground marker to local player's ball
    if let Some(ball) = state.balls.get(&network_role.local_player_id) {
        for (mut transform, mut visibility) in &mut marker_query {
            if ball.is_sunk {
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
                transform.translation.x = ball.position.x;
                transform.translation.y = 0.01;
                transform.translation.z = ball.position.z;
            }
        }
    }
}

fn aim_line_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    local_input: Res<GolfLocalInput>,
    mut dot_query: Query<(&AimDot, &mut Transform, &mut Visibility)>,
) {
    let state: Option<GolfState> = read_game_state(&active_game);
    let ball = state
        .as_ref()
        .and_then(|s| s.balls.get(&network_role.local_player_id));

    let show = ball.is_some_and(|b| !b.is_sunk && b.is_stopped());

    for (dot, mut transform, mut visibility) in &mut dot_query {
        if show {
            let ball = ball.unwrap();
            *visibility = Visibility::Visible;
            let dist = 0.6 + dot.0 as f32 * 0.5;
            let offset_x = local_input.aim_angle.cos() * dist;
            let offset_z = local_input.aim_angle.sin() * dist;
            transform.translation =
                Vec3::new(ball.position.x + offset_x, 0.15, ball.position.z + offset_z);
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn power_bar_system(
    local_input: Res<GolfLocalInput>,
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut fill_query: Query<(&mut Node, &mut BackgroundColor), With<PowerBarFill>>,
    mut label_query: Query<&mut Text, With<PowerBarLabel>>,
) {
    // Update label to show ball status
    let state: Option<GolfState> = read_game_state(&active_game);
    let ball_stopped = state.as_ref().is_some_and(|s| {
        s.balls
            .get(&network_role.local_player_id)
            .is_some_and(|b| b.is_stopped() && !b.is_sunk)
    });
    if let Ok(mut label) = label_query.single_mut() {
        **label = if ball_stopped {
            "POWER".to_string()
        } else {
            "Ball in motion...".to_string()
        };
    }

    if let Ok((mut node, mut bg)) = fill_query.single_mut() {
        node.width = Val::Percent(local_input.power * 100.0);
        // Gradient: green → yellow → red
        let p = local_input.power;
        let color = if p < 0.5 {
            let t = p * 2.0;
            Color::srgb(0.2 + t * 0.8, 0.8, 0.2)
        } else {
            let t = (p - 0.5) * 2.0;
            Color::srgb(1.0, 0.8 - t * 0.6, 0.2 - t * 0.15)
        };
        *bg = BackgroundColor(color);
    }
}

fn stroke_counter_system(
    active_game: Res<ActiveGame>,
    network_role: Res<NetworkRole>,
    mut text_query: Query<&mut Text, With<StrokeCounterText>>,
) {
    if let Ok(mut text) = text_query.single_mut() {
        let state: Option<GolfState> = read_game_state(&active_game);
        let strokes = state
            .and_then(|s| s.strokes.get(&network_role.local_player_id).copied())
            .unwrap_or(0);
        **text = format!("Strokes: {strokes}");
    }
}

fn hole_info_system(
    course_info: Option<Res<GolfCourseInfo>>,
    mut text_query: Query<&mut Text, With<HoleInfoText>>,
) {
    let Some(info) = course_info else {
        return;
    };
    if !info.is_changed() {
        return;
    }
    if let Ok(mut text) = text_query.single_mut() {
        **text = format!(
            "Hole {} of {} — {} — Par {}",
            info.hole_index + 1,
            info.total_holes,
            info.hole_name,
            info.par
        );
    }
}

fn scoreboard_system(
    active_game: Res<ActiveGame>,
    lobby: Res<crate::lobby::LobbyState>,
    mut text_query: Query<&mut Text, With<ScoreboardText>>,
) {
    if let Ok(mut text) = text_query.single_mut() {
        let state: Option<GolfState> = read_game_state(&active_game);
        if let Some(state) = state {
            let mut lines = Vec::new();
            for player in &lobby.players {
                if player.is_spectator {
                    continue;
                }
                let strokes = state.strokes.get(&player.id).copied().unwrap_or(0);
                let sunk = state.sunk_order.contains(&player.id);
                let status = if sunk { " [IN]" } else { "" };
                lines.push(format!("{}: {}{}", player.display_name, strokes, status));
            }
            **text = lines.join("\n");
        }
    }
}

/// Detect newly sunk balls and spawn a brief expanding flash at the hole.
#[allow(clippy::too_many_arguments)]
fn sink_flash_system(
    mut commands: Commands,
    active_game: Res<ActiveGame>,
    course_info: Option<Res<GolfCourseInfo>>,
    mut sunk_tracker: ResMut<SunkTracker>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut flash_query: Query<(Entity, &mut SinkFlash, &mut Transform)>,
) {
    // Update existing flashes
    for (entity, mut flash, mut transform) in &mut flash_query {
        flash.timer -= time.delta_secs();
        if flash.timer <= 0.0 {
            commands.entity(entity).despawn();
        } else {
            let scale = 1.0 + (0.5 - flash.timer) * 4.0;
            transform.scale = Vec3::splat(scale.max(0.1));
        }
    }

    // Detect new sinks
    let state: Option<GolfState> = read_game_state(&active_game);
    let Some(state) = state else {
        return;
    };
    let Some(info) = course_info else {
        return;
    };

    let courses = all_courses();
    let course = &courses[info.hole_index.min(courses.len() - 1)];

    for &pid in &state.sunk_order {
        if sunk_tracker.seen_sunk.insert(pid) {
            // New sink — spawn flash
            commands.spawn((
                GameEntity,
                SinkFlash { timer: 0.5 },
                Mesh3d(meshes.add(Sphere::new(HOLE_RADIUS * 1.5))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 1.0, 0.6, 0.6),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(course.hole_position.x, 0.3, course.hole_position.z),
            ));
        }
    }
}

fn cleanup_golf(mut commands: Commands) {
    commands.remove_resource::<GolfLocalInput>();
    commands.remove_resource::<SunkTracker>();
    // Note: GolfCourseInfo is preserved for BetweenRounds UI.
    // It's cleaned up in full_cleanup when returning to Lobby.
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::window::WindowResolution;

    fn test_window(w: u32, h: u32) -> Window {
        Window {
            resolution: WindowResolution::new(w, h),
            ..default()
        }
    }

    /// Camera transform matching the golf camera setup: looking down at the ball.
    fn golf_cam(ball_x: f32, ball_z: f32) -> Transform {
        Transform::from_xyz(ball_x, 15.0, ball_z - 2.0)
            .looking_at(Vec3::new(ball_x, 0.0, ball_z), Vec3::Y)
    }

    #[test]
    fn cursor_center_hits_near_look_target() {
        let window = test_window(1280, 720);
        let cam = golf_cam(6.0, 12.0);
        let center = Vec2::new(640.0, 360.0);

        let ground = cursor_to_ground(center, &window, &cam);
        assert!(ground.is_some(), "Center cursor should hit ground");
        let pt = ground.unwrap();
        // Should be near the look-at target (6, 0, 12)
        assert!(
            (pt.x - 6.0).abs() < 3.0,
            "Ground X should be near 6.0, got {}",
            pt.x
        );
        assert!(
            (pt.z - 12.0).abs() < 3.0,
            "Ground Z should be near 12.0, got {}",
            pt.z
        );
    }

    #[test]
    fn cursor_right_of_center_gives_positive_x_offset() {
        let window = test_window(1280, 720);
        let cam = golf_cam(6.0, 12.0);

        let center = cursor_to_ground(Vec2::new(640.0, 360.0), &window, &cam).unwrap();
        let right = cursor_to_ground(Vec2::new(960.0, 360.0), &window, &cam).unwrap();

        assert!(
            right.x > center.x,
            "Right cursor ({}) should map to greater X than center ({})",
            right.x,
            center.x
        );
    }

    #[test]
    fn cursor_left_of_center_gives_negative_x_offset() {
        let window = test_window(1280, 720);
        let cam = golf_cam(6.0, 12.0);

        let center = cursor_to_ground(Vec2::new(640.0, 360.0), &window, &cam).unwrap();
        let left = cursor_to_ground(Vec2::new(320.0, 360.0), &window, &cam).unwrap();

        assert!(
            left.x < center.x,
            "Left cursor ({}) should map to lesser X than center ({})",
            left.x,
            center.x
        );
    }

    #[test]
    fn cursor_above_center_gives_positive_z_offset() {
        let window = test_window(1280, 720);
        let cam = golf_cam(6.0, 12.0);

        // In screen coords, y=0 is top. Top of screen = further from camera = +Z.
        let center = cursor_to_ground(Vec2::new(640.0, 360.0), &window, &cam).unwrap();
        let top = cursor_to_ground(Vec2::new(640.0, 100.0), &window, &cam).unwrap();

        assert!(
            top.z > center.z,
            "Top-of-screen cursor (z={}) should map to greater Z than center (z={})",
            top.z,
            center.z
        );
    }

    #[test]
    fn cursor_below_center_gives_negative_z_offset() {
        let window = test_window(1280, 720);
        let cam = golf_cam(6.0, 12.0);

        let center = cursor_to_ground(Vec2::new(640.0, 360.0), &window, &cam).unwrap();
        let bottom = cursor_to_ground(Vec2::new(640.0, 620.0), &window, &cam).unwrap();

        assert!(
            bottom.z < center.z,
            "Bottom-of-screen cursor (z={}) should map to lesser Z than center (z={})",
            bottom.z,
            center.z
        );
    }

    #[test]
    fn different_camera_heights_consistent_direction() {
        let window = test_window(1280, 720);

        for height in [10.0, 15.0, 25.0] {
            let cam = Transform::from_xyz(6.0, height, 10.0)
                .looking_at(Vec3::new(6.0, 0.0, 12.0), Vec3::Y);

            let center = cursor_to_ground(Vec2::new(640.0, 360.0), &window, &cam).unwrap();
            let right = cursor_to_ground(Vec2::new(960.0, 360.0), &window, &cam).unwrap();

            assert!(
                right.x > center.x,
                "height={height}: right cursor X ({}) should exceed center X ({})",
                right.x,
                center.x
            );
        }
    }

    #[test]
    fn zero_dimension_window_returns_none() {
        let cam = golf_cam(6.0, 12.0);

        let zero_w = test_window(0, 720);
        assert!(
            cursor_to_ground(Vec2::new(0.0, 360.0), &zero_w, &cam).is_none(),
            "Zero-width window should return None"
        );

        let zero_h = test_window(1280, 0);
        assert!(
            cursor_to_ground(Vec2::new(640.0, 0.0), &zero_h, &cam).is_none(),
            "Zero-height window should return None"
        );
    }

    #[test]
    fn cursor_right_of_ball_gives_positive_x_stroke() {
        // End-to-end: cursor → ground → aim_angle → BallState::stroke → check vx>0
        let window = test_window(1280, 720);
        let ball_x = 6.0;
        let ball_z = 12.0;
        let cam = golf_cam(ball_x, ball_z);

        // Cursor to the right of center
        let ground = cursor_to_ground(Vec2::new(960.0, 360.0), &window, &cam).unwrap();

        let ball_pos = Vec3::new(ball_x, BALL_RADIUS, ball_z);
        let dx = ground.x - ball_pos.x;
        let dz = ground.z - ball_pos.z;
        let aim_angle = dz.atan2(dx);

        let mut ball = breakpoint_golf::physics::BallState::new(
            breakpoint_golf::course::Vec3::new(ball_x, 0.0, ball_z),
        );
        ball.stroke(aim_angle, 10.0);

        assert!(
            ball.velocity.x > 0.0,
            "Cursor right of ball should produce positive vx, got {}",
            ball.velocity.x
        );
    }
}
