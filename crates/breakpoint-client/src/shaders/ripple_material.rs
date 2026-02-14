use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;

/// Animated concentric-ring ripple material for golf holes.
#[derive(AsBindGroup, Clone, Asset, TypePath, Debug)]
pub struct RippleMaterial {
    /// Ripple color (RGBA, alpha controls overall opacity).
    #[uniform(0)]
    pub color: LinearRgba,
    /// x = time, y = ring_count, z = speed, w unused.
    #[uniform(1)]
    pub params: Vec4,
}

impl RippleMaterial {
    pub fn new(color: LinearRgba, ring_count: f32, speed: f32) -> Self {
        Self {
            color,
            params: Vec4::new(0.0, ring_count, speed, 0.0),
        }
    }
}

impl Material for RippleMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://breakpoint_client/shaders/ripple.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}
