use glam::{Mat4, Vec2, Vec3};

/// Camera modes for different games.
#[derive(Debug, Clone, Copy)]
pub enum CameraMode {
    /// Golf: follow ball from above.
    GolfFollow { ball_pos: Vec3 },
    /// Platformer: follow player in side-view.
    PlatformerFollow { player_pos: Vec2 },
    /// Laser tag: fixed top-down view.
    LaserTagFixed,
    /// Overview of the course (golf fallback).
    GolfOverview {
        center_x: f32,
        center_z: f32,
        extent: f32,
    },
    /// Lobby: no 3D camera needed (UI only).
    None,
}

/// Camera with perspective projection.
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    mode: CameraMode,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3::new(10.0, 30.0, -5.0),
            target: Vec3::new(10.0, 0.0, 15.0),
            up: Vec3::Y,
            fov: std::f32::consts::FRAC_PI_4, // 45 degrees
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 200.0,
            mode: CameraMode::None,
        }
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position, self.target, self.up)
    }

    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh_gl(self.fov, self.aspect, self.near, self.far)
    }

    pub fn view_projection(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    pub fn set_mode(&mut self, mode: CameraMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> &CameraMode {
        &self.mode
    }

    /// Update camera position based on current mode with smooth lerp.
    pub fn update(&mut self, dt: f32) {
        let lerp_factor = (5.0 * dt).min(1.0);

        match self.mode {
            CameraMode::GolfFollow { ball_pos } => {
                let camera_height = 15.0;
                let offset_z = -2.0;
                let target_pos = Vec3::new(ball_pos.x, camera_height, ball_pos.z + offset_z);
                let look_at = Vec3::new(ball_pos.x, 0.0, ball_pos.z);

                self.position = self.position.lerp(target_pos, lerp_factor);
                self.target = self.target.lerp(look_at, lerp_factor);
                self.up = Vec3::Y;
            },
            CameraMode::GolfOverview {
                center_x,
                center_z,
                extent,
            } => {
                let h = extent * 1.1;
                let offset_z = -extent * 0.15;
                self.position = Vec3::new(center_x, h, center_z + offset_z);
                self.target = Vec3::new(center_x, 0.0, center_z);
                self.up = Vec3::Y;
            },
            CameraMode::PlatformerFollow { player_pos } => {
                let camera_z = -25.0;
                let look_y_offset = 3.0;
                let target_pos = Vec3::new(player_pos.x, player_pos.y + look_y_offset, camera_z);
                let look_at = Vec3::new(player_pos.x, player_pos.y + look_y_offset, 0.0);

                self.position = self.position.lerp(target_pos, lerp_factor);
                self.target = self.target.lerp(look_at, lerp_factor);
                self.up = Vec3::Y;
            },
            CameraMode::LaserTagFixed => {
                self.position = Vec3::new(25.0, 62.0, 25.0);
                self.target = Vec3::new(25.0, 0.0, 25.0);
                self.up = Vec3::Z;
            },
            CameraMode::None => {},
        }
    }

    /// Project a screen-space cursor position onto the Y=0 ground plane.
    /// Uses the camera's actual FOV and aspect ratio — no hardcoded constants.
    pub fn screen_to_ground(&self, cursor: Vec2, viewport: Vec2) -> Option<Vec3> {
        if viewport.x < 1.0 || viewport.y < 1.0 {
            return None;
        }

        // Cursor to NDC: x in [-1,1], y in [-1,1] (bottom to top)
        let ndc_x = (cursor.x / viewport.x) * 2.0 - 1.0;
        let ndc_y = 1.0 - (cursor.y / viewport.y) * 2.0;

        let half_v = (self.fov * 0.5).tan();
        let half_h = half_v * self.aspect;

        // Build world-space ray from camera axes
        let view = self.view_matrix();
        // Extract axes from inverse view matrix (camera-to-world)
        let inv_view = view.inverse();
        let right = inv_view.col(0).truncate();
        let up = inv_view.col(1).truncate();
        let forward = -inv_view.col(2).truncate(); // Camera looks along -Z in view space

        let ray_dir = (forward + right * (ndc_x * half_h) + up * (ndc_y * half_v)).normalize();

        // Intersect with Y=0 plane
        if ray_dir.y.abs() < 1e-6 {
            return None;
        }
        let t = -self.position.y / ray_dir.y;
        if t <= 0.0 {
            return None;
        }
        Some(self.position + ray_dir * t)
    }

    /// Apply a shake offset to the camera position (for screen shake effect).
    pub fn apply_shake(&mut self, offset: Vec3) {
        self.position += offset;
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_view_projection_not_identity() {
        let cam = Camera::new();
        let vp = cam.view_projection();
        assert!(!vp.abs_diff_eq(Mat4::IDENTITY, 1e-6));
    }

    #[test]
    fn screen_center_hits_ground() {
        let mut cam = Camera::new();
        cam.position = Vec3::new(0.0, 10.0, 0.0);
        cam.target = Vec3::new(0.0, 0.0, 10.0);
        cam.aspect = 1.0;

        let viewport = Vec2::new(800.0, 600.0);
        let center = Vec2::new(400.0, 300.0);
        let hit = cam.screen_to_ground(center, viewport);
        assert!(hit.is_some(), "Center of screen should hit ground plane");
        let p = hit.unwrap();
        assert!(
            p.y.abs() < 0.01,
            "Hit point should be on Y=0 plane, got y={}",
            p.y
        );
    }

    #[test]
    fn screen_to_ground_zero_viewport_returns_none() {
        let cam = Camera::new();
        assert!(
            cam.screen_to_ground(Vec2::new(100.0, 100.0), Vec2::ZERO)
                .is_none()
        );
    }

    #[test]
    fn camera_update_lerps() {
        let mut cam = Camera::new();
        cam.position = Vec3::new(0.0, 10.0, 0.0);
        cam.set_mode(CameraMode::GolfFollow {
            ball_pos: Vec3::new(20.0, 0.0, 20.0),
        });
        let before = cam.position;
        cam.update(0.1);
        assert_ne!(cam.position, before, "Camera should move toward target");
    }

    #[test]
    fn camera_lasertag_fixed_position() {
        let mut cam = Camera::new();
        cam.set_mode(CameraMode::LaserTagFixed);
        cam.update(1.0);
        assert!((cam.position.x - 25.0).abs() < 0.01);
        assert!((cam.position.y - 62.0).abs() < 0.01);
    }

    #[test]
    fn projection_matrix_is_perspective() {
        let cam = Camera::new();
        let proj = cam.projection_matrix();
        // Perspective matrix has non-zero values in specific positions
        assert!(proj.col(2).w.abs() > 0.0, "Should be a perspective matrix");
    }

    /// Verify screen_to_ground matches the expected behavior for a top-down camera.
    #[test]
    fn screen_to_ground_top_down_camera() {
        let mut cam = Camera::new();
        cam.position = Vec3::new(5.0, 15.0, 5.0);
        cam.target = Vec3::new(5.0, 0.0, 5.0);
        cam.up = Vec3::Z; // top-down needs different up
        cam.aspect = 1.0;

        let viewport = Vec2::new(800.0, 800.0);
        let center = Vec2::new(400.0, 400.0);
        let hit = cam.screen_to_ground(center, viewport);
        assert!(hit.is_some());
        let p = hit.unwrap();
        // Should be close to (5, 0, 5) - directly below camera
        assert!((p.x - 5.0).abs() < 1.0, "Expected x near 5.0, got {}", p.x);
    }
}
