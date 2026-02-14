use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;

/// Simple UV-based two-color gradient for golf course floors.
#[derive(AsBindGroup, Clone, Asset, TypePath, Debug)]
pub struct GradientMaterial {
    /// Color at UV v=0 (spawn end).
    #[uniform(0)]
    pub color_start: LinearRgba,
    /// Color at UV v=1 (hole end).
    #[uniform(1)]
    pub color_end: LinearRgba,
}

impl GradientMaterial {
    pub fn new(start: LinearRgba, end: LinearRgba) -> Self {
        Self {
            color_start: start,
            color_end: end,
        }
    }
}

impl Material for GradientMaterial {
    fn fragment_shader() -> bevy::shader::ShaderRef {
        "embedded://breakpoint_client/shaders/gradient.wgsl".into()
    }
}
