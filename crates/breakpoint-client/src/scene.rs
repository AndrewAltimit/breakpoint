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
    Sphere { segments: u16 },
    Cylinder { segments: u16 },
    Cuboid,
    Plane,
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
}

impl Scene {
    pub fn new() -> Self {
        Self {
            objects: Vec::with_capacity(512),
            next_id: 1,
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

    /// Clear all objects.
    pub fn clear(&mut self) {
        self.objects.clear();
        self.next_id = 1;
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
