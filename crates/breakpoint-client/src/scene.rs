use glam::{Mat4, Quat, Vec3, Vec4};

/// Unique identifier for a render object.
pub type ObjectId = u32;

/// Transform for positioning objects in world space.
#[derive(Debug, Clone, Copy)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl Transform {
    pub fn from_xyz(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: Vec3::new(x, y, z),
            ..Default::default()
        }
    }

    pub fn from_translation(t: Vec3) -> Self {
        Self {
            translation: t,
            ..Default::default()
        }
    }

    pub fn with_scale(mut self, scale: Vec3) -> Self {
        self.scale = scale;
        self
    }

    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Build the model matrix (Translation * Rotation * Scale).
    pub fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}

/// Mesh primitive types.
#[derive(Debug, Clone, Copy)]
pub enum MeshType {
    Sphere {
        segments: u16,
    },
    Cylinder {
        segments: u16,
    },
    Cuboid,
    Plane,
    /// XY-plane billboard quad (faces +Z, toward camera at Z<0).
    Quad,
}

/// Blend mode for sprite rendering (MBAACC-style).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Additive,
    Subtractive,
}

/// Material types matching the GLSL shader programs.
#[derive(Debug, Clone, Copy)]
pub enum MaterialType {
    Unlit {
        color: Vec4,
    },
    Gradient {
        start: Vec4,
        end: Vec4,
    },
    Ripple {
        color: Vec4,
        ring_count: f32,
        speed: f32,
    },
    Glow {
        color: Vec4,
        intensity: f32,
    },
    TronWall {
        color: Vec4,
        intensity: f32,
    },
    /// Textured sprite from a texture atlas.
    Sprite {
        atlas_id: u8,
        sprite_rect: Vec4,
        tint: Vec4,
        flip_x: bool,
        /// Dissolve amount: 0.0 = solid, 1.0 = fully dissolved. Used for death effects.
        dissolve: f32,
        /// Outline width: >0.0 enables dark pixel outline (MBAACC-style).
        outline: f32,
        /// Blend mode: Normal, Additive, or Subtractive.
        blend_mode: BlendMode,
    },
    /// Parallax background layer (scrolling textured quad).
    Parallax {
        atlas_id: u8,
        /// UV rect for the layer in the background texture (v0..v1 row).
        layer_rect: Vec4,
        /// Scroll speed multiplier (0.0 = static, 1.0 = camera speed).
        scroll_factor: f32,
        tint: Vec4,
    },
    /// Animated water with waves, caustics, and transparency.
    Water {
        color: Vec4,
        depth: f32,
        wave_speed: f32,
    },
    /// Whip attack trail arc effect.
    WhipTrail {
        progress: f32,
        color: Vec4,
    },
    /// Anime-style slash arc VFX.
    SlashArc {
        progress: f32,
        angle: f32,
        color: Vec4,
    },
    /// Rotating magic circle VFX (power-up activation).
    MagicCircle {
        rotation: f32,
        pulse: f32,
        color: Vec4,
    },
    /// Volumetric god rays from stained glass or bright light sources.
    GodRays {
        intensity: f32,
        color: Vec4,
    },
    /// Ground fog layer with scrolling noise.
    FogLayer {
        density: f32,
        color: Vec4,
    },
    /// Procedural health bar (fill amount = intensity).
    HealthBar {
        fill: f32,
        color: Vec4,
    },
}

/// Lighting information for the scene (torch lights, ambient).
pub struct SceneLighting {
    /// Up to 32 lights: (x, y, intensity, radius).
    pub lights: Vec<[f32; 4]>,
    /// Per-light color: (r, g, b, type). Type 0=point, 1=directional.
    pub light_colors: Vec<[f32; 4]>,
    /// Ambient light level (0.0 = pitch black, 1.0 = fully lit).
    pub ambient: f32,
    /// Per-room ambient color (RGB). Defaults to neutral white.
    pub ambient_color: [f32; 3],
    /// Per-room color grading: shadow tint (RGB, 1.0=neutral).
    pub grade_shadows: [f32; 3],
    /// Per-room color grading: highlight tint (RGB, 1.0=neutral).
    pub grade_highlights: [f32; 3],
    /// Per-room contrast (1.0=neutral).
    pub grade_contrast: f32,
    /// Per-room saturation (1.0=neutral).
    pub saturation: f32,
    /// GBA-style color ramp: shadow color (RGB). Zero = disabled.
    pub ramp_shadow: [f32; 3],
    /// GBA-style color ramp: midtone color (RGB).
    pub ramp_mid: [f32; 3],
    /// GBA-style color ramp: highlight color (RGB).
    pub ramp_highlight: [f32; 3],
    /// GBA-style posterization bit depth (0.0=off, 31.0=5-bit GBA).
    pub posterize: f32,
    /// Per-room fog color (RGB). Used by sprite shader ground fog.
    pub fog_color: [f32; 3],
}

/// A renderable object in the scene.
pub struct RenderObject {
    pub id: ObjectId,
    pub mesh: MeshType,
    pub material: MaterialType,
    pub transform: Transform,
    pub visible: bool,
}

/// Flat scene graph — no hierarchy needed for this project.
pub struct Scene {
    objects: Vec<RenderObject>,
    next_id: ObjectId,
    /// Scene lighting (set by game-specific render code, read by renderer).
    pub lighting: SceneLighting,
    /// Number of objects in the static layer (tiles, background).
    /// Objects up to this count are preserved by `clear_dynamic()`.
    static_count: usize,
    /// Pre-built sprite batch vertex data (10 floats/vertex × 6 vertices/sprite).
    /// Written by game render code via `add_batch_sprite()`, consumed directly by
    /// the renderer — bypasses RenderObject creation, frustum culling, and sorting.
    pub batch_normal: Vec<f32>,
    pub batch_additive: Vec<f32>,
    pub batch_subtractive: Vec<f32>,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::with_capacity(2048),
            next_id: 1,
            static_count: 0,
            // Pre-allocate batch buffers: ~600 tiles × 10 floats × 6 verts = 36000 floats.
            batch_normal: Vec::with_capacity(40_000),
            batch_additive: Vec::with_capacity(4_000),
            batch_subtractive: Vec::with_capacity(4_000),
            lighting: SceneLighting {
                lights: Vec::new(),
                light_colors: Vec::new(),
                ambient: 1.0,
                ambient_color: [1.0, 1.0, 1.0],
                grade_shadows: [1.0, 1.0, 1.0],
                grade_highlights: [1.0, 1.0, 1.0],
                grade_contrast: 1.0,
                saturation: 1.0,
                ramp_shadow: [0.0, 0.0, 0.0],
                ramp_mid: [0.0, 0.0, 0.0],
                ramp_highlight: [0.0, 0.0, 0.0],
                posterize: 0.0,
                fog_color: [0.12, 0.10, 0.18],
            },
        }
    }

    /// Add an object to the scene, returning its ID.
    pub fn add(
        &mut self,
        mesh: MeshType,
        material: MaterialType,
        transform: Transform,
    ) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        self.objects.push(RenderObject {
            id,
            mesh,
            material,
            transform,
            visible: true,
        });
        id
    }

    /// Get a mutable reference to an object by ID.
    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut RenderObject> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    /// Get an immutable reference to an object by ID.
    pub fn get(&self, id: ObjectId) -> Option<&RenderObject> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Remove an object by ID.
    pub fn remove(&mut self, id: ObjectId) {
        self.objects.retain(|o| o.id != id);
    }

    /// Add a sprite directly to the pre-built batch buffer (10 floats × 6 vertices).
    /// Bypasses RenderObject creation for batchable sprites (no dissolve).
    #[allow(clippy::too_many_arguments)]
    pub fn add_batch_sprite(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        w: f32,
        h: f32,
        sprite_rect: Vec4,
        tint: Vec4,
        flip_x: bool,
        outline: f32,
        blend_mode: BlendMode,
    ) {
        let half_x = w * 0.5;
        let half_y = h * 0.5;
        let x0 = x - half_x;
        let x1 = x + half_x;
        let y0 = y - half_y;
        let y1 = y + half_y;

        let (u0, u1) = if flip_x {
            (sprite_rect.z, sprite_rect.x)
        } else {
            (sprite_rect.x, sprite_rect.z)
        };
        let v0 = sprite_rect.w; // bottom
        let v1 = sprite_rect.y; // top

        let tr = tint.x;
        let tg = tint.y;
        let tb = tint.z;
        let ta = tint.w;
        let ol = outline;

        let buf = match blend_mode {
            BlendMode::Normal => &mut self.batch_normal,
            BlendMode::Additive => &mut self.batch_additive,
            BlendMode::Subtractive => &mut self.batch_subtractive,
        };
        // 10 floats per vertex: pos(3) + uv(2) + tint(4) + outline(1)
        // Triangle 1: bottom-left, top-right, bottom-right
        buf.extend_from_slice(&[x0, y0, z, u0, v0, tr, tg, tb, ta, ol]);
        buf.extend_from_slice(&[x1, y1, z, u1, v1, tr, tg, tb, ta, ol]);
        buf.extend_from_slice(&[x1, y0, z, u1, v0, tr, tg, tb, ta, ol]);
        // Triangle 2: bottom-left, top-left, top-right
        buf.extend_from_slice(&[x0, y0, z, u0, v0, tr, tg, tb, ta, ol]);
        buf.extend_from_slice(&[x0, y1, z, u0, v1, tr, tg, tb, ta, ol]);
        buf.extend_from_slice(&[x1, y1, z, u1, v1, tr, tg, tb, ta, ol]);
    }

    /// Clear all objects, preserving allocated capacity for reuse.
    pub fn clear(&mut self) {
        self.objects.clear();
        // Reset to 1: IDs only need frame-local uniqueness since the scene
        // is rebuilt each frame. Prevents u32 overflow at 800 objs * 60fps.
        self.next_id = 1;
        self.static_count = 0;
        self.batch_normal.clear();
        self.batch_additive.clear();
        self.batch_subtractive.clear();
    }

    /// Mark the current objects as the static layer (tiles, background).
    /// Subsequent `clear_dynamic()` calls will preserve these objects.
    pub fn mark_static(&mut self) {
        self.static_count = self.objects.len();
    }

    /// Clear only dynamic objects (players, enemies, particles) added after
    /// the last `mark_static()` call, preserving static tile geometry.
    pub fn clear_dynamic(&mut self) {
        self.objects.truncate(self.static_count);
        // Reset next_id from the static boundary to prevent overflow.
        self.next_id = self.static_count as ObjectId + 1;
        // Batch buffers are rebuilt each frame.
        self.batch_normal.clear();
        self.batch_additive.clear();
        self.batch_subtractive.clear();
    }

    /// Whether static objects have been marked (tiles are cached).
    pub fn has_static(&self) -> bool {
        self.static_count > 0
    }

    /// Iterate over all visible objects.
    pub fn visible_objects(&self) -> impl Iterator<Item = &RenderObject> {
        self.objects.iter().filter(|o| o.visible)
    }

    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_add_and_get() {
        let mut scene = Scene::new();
        let id = scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit {
                color: Vec4::new(1.0, 0.0, 0.0, 1.0),
            },
            Transform::from_xyz(1.0, 2.0, 3.0),
        );
        assert!(scene.get(id).is_some());
        assert_eq!(scene.object_count(), 1);
    }

    #[test]
    fn scene_remove() {
        let mut scene = Scene::new();
        let id = scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color: Vec4::ONE },
            Transform::default(),
        );
        scene.remove(id);
        assert!(scene.get(id).is_none());
        assert_eq!(scene.object_count(), 0);
    }

    #[test]
    fn scene_clear() {
        let mut scene = Scene::new();
        for _ in 0..10 {
            scene.add(
                MeshType::Cuboid,
                MaterialType::Unlit { color: Vec4::ONE },
                Transform::default(),
            );
        }
        assert_eq!(scene.object_count(), 10);
        scene.clear();
        assert_eq!(scene.object_count(), 0);
        // IDs reset after clear (frame-local uniqueness, prevents u32 overflow)
        let id = scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color: Vec4::ONE },
            Transform::default(),
        );
        assert_eq!(id, 1);
    }

    #[test]
    fn scene_visible_objects_filters() {
        let mut scene = Scene::new();
        let id1 = scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color: Vec4::ONE },
            Transform::default(),
        );
        let _id2 = scene.add(
            MeshType::Cuboid,
            MaterialType::Unlit { color: Vec4::ONE },
            Transform::default(),
        );
        scene.get_mut(id1).unwrap().visible = false;
        assert_eq!(scene.visible_objects().count(), 1);
    }

    #[test]
    fn transform_matrix_identity() {
        let t = Transform::default();
        let m = t.matrix();
        assert!((m - Mat4::IDENTITY).abs_diff_eq(Mat4::ZERO, 1e-6));
    }

    #[test]
    fn transform_matrix_translation() {
        let t = Transform::from_xyz(3.0, 4.0, 5.0);
        let m = t.matrix();
        let col3 = m.col(3);
        assert!((col3.x - 3.0).abs() < 1e-6);
        assert!((col3.y - 4.0).abs() < 1e-6);
        assert!((col3.z - 5.0).abs() < 1e-6);
    }
}
