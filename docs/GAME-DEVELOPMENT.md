# Breakpoint Game Development Guide

This guide explains how to add a new game to the Breakpoint platform.

## The BreakpointGame Trait

Every game implements the `BreakpointGame` trait defined in `breakpoint-core`:

```rust
pub trait BreakpointGame: Send + Sync {
    // Required
    fn metadata(&self) -> GameMetadata;
    fn init(&mut self, players: &[Player], config: &GameConfig);
    fn update(&mut self, dt: f32, inputs: &PlayerInputs) -> Vec<GameEvent>;
    fn serialize_state(&self) -> Vec<u8>;
    fn apply_state(&mut self, state: &[u8]);
    fn serialize_input(&self, player_id: PlayerId) -> Vec<u8>;
    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]);
    fn player_joined(&mut self, player: &Player);
    fn player_left(&mut self, player_id: PlayerId);
    fn pause(&mut self);
    fn resume(&mut self);
    fn is_round_complete(&self) -> bool;
    fn round_results(&self) -> Vec<PlayerScore>;

    // Optional (with defaults)
    fn tick_rate(&self) -> f32 { 10.0 }
    fn supports_pause(&self) -> bool { true }
}
```

The platform handles networking, lobby, overlay, audio, and player management. Your game only implements game-specific simulation and rendering.

## Step-by-Step: Adding a New Game

### 1. Create the Game Crate

```bash
mkdir -p crates/games/breakpoint-mygame/src
```

Create `crates/games/breakpoint-mygame/Cargo.toml`:

```toml
[package]
name = "breakpoint-mygame"
description = "My Game for Breakpoint"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
breakpoint-core = { path = "../../breakpoint-core" }
serde.workspace = true
rmp-serde.workspace = true

[lints]
workspace = true
```

### 2. Implement the Game

Create `crates/games/breakpoint-mygame/src/lib.rs`:

```rust
use std::collections::HashMap;
use breakpoint_core::game_trait::*;
use breakpoint_core::player::Player;
use serde::{Deserialize, Serialize};

/// Your game's state.
#[derive(Default, Serialize, Deserialize)]
pub struct MyGame {
    scores: HashMap<PlayerId, i32>,
    round_complete: bool,
    paused: bool,
}

/// Player input for your game.
#[derive(Default, Serialize, Deserialize)]
struct MyInput {
    // Define input fields here
    action: bool,
}

impl BreakpointGame for MyGame {
    fn metadata(&self) -> GameMetadata {
        GameMetadata {
            name: "My Game".to_string(),
            description: "A custom Breakpoint game".to_string(),
            min_players: 2,
            max_players: 8,
            estimated_round_duration: std::time::Duration::from_secs(120),
        }
    }

    fn init(&mut self, players: &[Player], _config: &GameConfig) {
        self.scores.clear();
        self.round_complete = false;
        self.paused = false;
        for p in players {
            self.scores.insert(p.id, 0);
        }
    }

    fn update(&mut self, _dt: f32, _inputs: &PlayerInputs) -> Vec<GameEvent> {
        if self.paused {
            return vec![];
        }
        // Game logic here
        vec![]
    }

    fn serialize_state(&self) -> Vec<u8> {
        rmp_serde::to_vec(self).unwrap_or_default()
    }

    fn apply_state(&mut self, state: &[u8]) {
        if let Ok(s) = rmp_serde::from_slice::<MyGame>(state) {
            *self = s;
        }
    }

    fn serialize_input(&self, _player_id: PlayerId) -> Vec<u8> {
        rmp_serde::to_vec(&MyInput::default()).unwrap_or_default()
    }

    fn apply_input(&mut self, _player_id: PlayerId, input: &[u8]) {
        if let Ok(_input) = rmp_serde::from_slice::<MyInput>(input) {
            // Apply input to game state
        }
    }

    fn player_joined(&mut self, player: &Player) {
        self.scores.entry(player.id).or_insert(0);
    }

    fn player_left(&mut self, player_id: PlayerId) {
        self.scores.remove(&player_id);
    }

    fn tick_rate(&self) -> f32 {
        15.0 // Hz
    }

    fn pause(&mut self) {
        self.paused = true;
    }

    fn resume(&mut self) {
        self.paused = false;
    }

    fn is_round_complete(&self) -> bool {
        self.round_complete
    }

    fn round_results(&self) -> Vec<PlayerScore> {
        self.scores
            .iter()
            .map(|(&player_id, &score)| PlayerScore { player_id, score })
            .collect()
    }
}
```

### 3. Register in the Workspace

Add to `Cargo.toml` workspace members:

```toml
[workspace]
members = [
    # ... existing members
    "crates/games/breakpoint-mygame",
]
```

### 4. Add to the Client

Add the dependency to `crates/breakpoint-client/Cargo.toml`:

```toml
[dependencies]
breakpoint-mygame = { path = "../games/breakpoint-mygame", optional = true }

[features]
default = ["golf", "platformer", "lasertag", "mygame"]
mygame = ["dep:breakpoint-mygame"]
```

Then add the game module in the client's `game/` directory and register it in the game selection system.

## Key Concepts

### Host-Authoritative Model

The host browser runs the authoritative simulation. Clients send inputs and receive state:

1. **Client** captures input each frame via `serialize_input()`
2. **Host** receives input and applies it via `apply_input()`
3. **Host** runs `update()` with all collected inputs
4. **Host** broadcasts state via `serialize_state()`
5. **Clients** apply state via `apply_state()` and render

### Tick Rate

`tick_rate()` controls how many times per second the game state is synchronized over the network:

- **10 Hz** — Turn-based or slow games (mini-golf)
- **15 Hz** — Moderate action (platformer)
- **20 Hz** — Fast action (laser tag)

Higher rates consume more bandwidth. Choose the minimum that feels responsive for your game.

### Game Events

`update()` returns `Vec<GameEvent>` for scoring and round management:

```rust
pub enum GameEvent {
    ScoreUpdate { player_id: PlayerId, score: i32 },
    RoundComplete,
}
```

The platform uses these to update the between-rounds screen and final scores.

### Pause Support

When the overlay issues a critical alert, it calls `pause()`. Resume with `resume()`. If your game cannot support pausing (e.g., real-time competitive), return `false` from `supports_pause()`.

### Late Join

`player_joined()` is called when a player connects mid-game. Initialize their state and add them to the simulation. The full current state will be sent to them via `serialize_state()`.

## Example: Mini-Golf

See `crates/games/breakpoint-golf/` for a complete implementation. Key patterns:

- Course data stored as serializable structs
- Physics simulation in `update()` with delta time
- Ball positions and velocities serialized as game state
- Aim angle and power serialized as player input
- Round completes when all players sink their ball or time expires
- Scoring based on stroke count with bonuses for first-to-sink
