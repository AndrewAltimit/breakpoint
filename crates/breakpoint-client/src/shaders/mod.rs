pub mod glow_material;
pub mod gradient_material;
pub mod ripple_material;

use bevy::prelude::*;

use glow_material::GlowMaterial;
use gradient_material::GradientMaterial;
use ripple_material::RippleMaterial;

pub struct ShadersPlugin;

impl Plugin for ShadersPlugin {
    fn build(&self, app: &mut App) {
        // Embed WGSL shader assets
        bevy::asset::embedded_asset!(app, "glow.wgsl");
        bevy::asset::embedded_asset!(app, "ripple.wgsl");
        bevy::asset::embedded_asset!(app, "gradient.wgsl");

        // Register material plugins
        app.add_plugins((
            MaterialPlugin::<GlowMaterial>::default(),
            MaterialPlugin::<RippleMaterial>::default(),
            MaterialPlugin::<GradientMaterial>::default(),
        ));
    }
}
