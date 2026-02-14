pub mod particles;
pub mod screen_shake;
pub mod squash_stretch;

use bevy::prelude::*;

use crate::app::AppState;

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<screen_shake::ScreenShake>()
            .add_systems(
                Update,
                screen_shake::screen_shake_decay_system.run_if(in_state(AppState::InGame)),
            );
    }
}
