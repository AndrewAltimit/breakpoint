use serde::{Deserialize, Serialize};

use crate::game_trait::GameMetadata;

/// Unique identifier for a registered game type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GameId(pub String);

/// A registered game entry in the game catalog.
#[derive(Debug, Clone)]
pub struct GameEntry {
    pub id: GameId,
    pub metadata: GameMetadata,
}
