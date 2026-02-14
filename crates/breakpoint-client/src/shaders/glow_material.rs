use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;

/// Soft-glow material for laser beams — bright center fading to transparent edges.
#[derive(AsBindGroup, Clone, Asset, TypePath, Debug)]
pub struct GlowMaterial {
    /// Beam color (RGBA).
    #[uniform(0)]
    pub color: LinearRgba,
    /// x = intensity, y = alpha, z/w unused.
    #[uniform(1)]
    pub params: Vec4,
}

impl GlowMaterial {
    pub fn new(color: LinearRgba, intensity: f32, alpha: f32) -> Self {
        Self {
            color,
            params: Vec4::new(intensity, alpha, 0.0, 0.0),
        }
    }
}

impl Material for GlowMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://breakpoint_client/shaders/glow.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}
