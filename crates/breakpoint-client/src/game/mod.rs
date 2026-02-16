#[cfg(feature = "golf")]
pub mod golf_input;
#[cfg(feature = "golf")]
pub mod golf_render;
#[cfg(feature = "lasertag")]
pub mod lasertag_input;
#[cfg(feature = "lasertag")]
pub mod lasertag_render;
#[cfg(feature = "platformer")]
pub mod platformer_input;
#[cfg(feature = "platformer")]
pub mod platformer_render;
#[cfg(feature = "tron")]
pub mod tron_input;
#[cfg(feature = "tron")]
pub mod tron_render;

use std::collections::HashMap;

use breakpoint_core::game_trait::{BreakpointGame, GameId};
use breakpoint_core::net::messages::PlayerInputMsg;
use breakpoint_core::net::protocol::encode_client_message;

use crate::app::ActiveGame;
use crate::app::NetworkRole;
use crate::net_client::WsClient;

/// Factory function type: creates a new game instance.
type GameFactory = fn() -> Box<dyn BreakpointGame>;

/// Registry mapping game IDs to factory functions.
#[derive(Default)]
pub struct GameRegistry {
    factories: HashMap<GameId, GameFactory>,
}

impl GameRegistry {
    pub fn register(&mut self, game_id: GameId, factory: GameFactory) {
        self.factories.insert(game_id, factory);
    }

    pub fn create(&self, game_id: GameId) -> Option<Box<dyn BreakpointGame>> {
        self.factories.get(&game_id).map(|f| f())
    }
}

/// Create a fully populated game registry.
pub fn create_registry() -> GameRegistry {
    let mut registry = GameRegistry::default();
    #[cfg(feature = "golf")]
    registry.register(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
    #[cfg(feature = "platformer")]
    registry.register(GameId::Platformer, || {
        Box::new(breakpoint_platformer::PlatformRacer::new())
    });
    #[cfg(feature = "lasertag")]
    registry.register(GameId::LaserTag, || {
        Box::new(breakpoint_lasertag::LaserTagArena::new())
    });
    #[cfg(feature = "tron")]
    registry.register(
        GameId::Tron,
        || Box::new(breakpoint_tron::TronCycles::new()),
    );
    registry
}

/// Serialize and send player input to the server via WebSocket.
pub fn send_player_input(
    input: &impl serde::Serialize,
    active_game: &mut ActiveGame,
    network_role: &NetworkRole,
    ws_client: &WsClient,
) {
    if let Ok(data) = rmp_serde::to_vec(input) {
        let msg = breakpoint_core::net::messages::ClientMessage::PlayerInput(PlayerInputMsg {
            player_id: network_role.local_player_id,
            tick: active_game.tick,
            input_data: data,
        });
        if let Ok(encoded) = encode_client_message(&msg) {
            let _ = ws_client.send(&encoded);
        }
    }
}

/// Deserialize the current game state from the active game.
pub fn read_game_state<S: serde::de::DeserializeOwned>(active_game: &ActiveGame) -> Option<S> {
    let bytes = if let Some(ref cached) = active_game.cached_state_bytes {
        cached.as_slice()
    } else {
        return rmp_serde::from_slice(&active_game.game.serialize_state()).ok();
    };
    rmp_serde::from_slice(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_registry_register_and_create() {
        let mut registry = GameRegistry::default();
        assert!(registry.create(GameId::Golf).is_none());

        registry.register(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
        assert!(registry.create(GameId::Golf).is_some());
        assert!(registry.create(GameId::Platformer).is_none());
    }

    #[test]
    fn game_registry_multiple_games() {
        let mut registry = GameRegistry::default();
        registry.register(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
        registry.register(GameId::Platformer, || {
            Box::new(breakpoint_platformer::PlatformRacer::new())
        });
        assert!(registry.create(GameId::Golf).is_some());
        assert!(registry.create(GameId::Platformer).is_some());
    }
}
