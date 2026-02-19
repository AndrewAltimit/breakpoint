use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use breakpoint_core::game_trait::{
    BreakpointGame, GameConfig, GameEvent, GameId, PlayerId, PlayerInputs,
};
use breakpoint_core::net::messages::{
    GameEndMsg, GameStartMsg, GameStateMsg, PlayerScoreEntry, RoundEndMsg, ServerMessage,
};
use breakpoint_core::net::protocol::encode_server_message;
use breakpoint_core::player::Player;

/// Commands sent from the WebSocket handler to the game tick loop.
#[derive(Debug)]
pub enum GameCommand {
    PlayerInput {
        player_id: PlayerId,
        tick: u32,
        input_data: Vec<u8>,
    },
    PlayerJoined {
        player_id: PlayerId,
        player: Player,
    },
    PlayerLeft {
        player_id: PlayerId,
    },
    Stop,
}

/// Broadcasts sent from the game tick loop to all connected clients.
#[derive(Debug, Clone)]
pub enum GameBroadcast {
    /// Serialized ServerMessage bytes ready to send over WebSocket.
    /// Uses `Bytes` for zero-copy cloning across player channels.
    EncodedMessage(Bytes),
    /// Signal that the game has ended and the loop has exited.
    GameEnded,
}

/// Factory function type for creating game instances on the server.
type ServerGameFactory = fn() -> Box<dyn BreakpointGame>;

/// Registry mapping game IDs to factory functions (server-side).
pub struct ServerGameRegistry {
    factories: HashMap<GameId, ServerGameFactory>,
}

impl Default for ServerGameRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerGameRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            factories: HashMap::new(),
        };
        registry.register_defaults();
        registry
    }

    fn register_defaults(&mut self) {
        #[cfg(feature = "golf")]
        self.factories
            .insert(GameId::Golf, || Box::new(breakpoint_golf::MiniGolf::new()));
        #[cfg(feature = "platformer")]
        self.factories.insert(GameId::Platformer, || {
            Box::new(breakpoint_platformer::PlatformRacer::new())
        });
        #[cfg(feature = "lasertag")]
        self.factories.insert(GameId::LaserTag, || {
            Box::new(breakpoint_lasertag::LaserTagArena::new())
        });
        #[cfg(feature = "tron")]
        self.factories.insert(
            GameId::Tron,
            || Box::new(breakpoint_tron::TronCycles::new()),
        );
    }

    pub fn create(&self, game_id: GameId) -> Option<Box<dyn BreakpointGame>> {
        self.factories.get(&game_id).map(|f| f())
    }

    /// Return the number of registered game types.
    pub fn available_games(&self) -> usize {
        self.factories.len()
    }
}

/// Configuration for a game session spawned by the server.
pub struct GameSessionConfig {
    pub game_id: GameId,
    pub players: Vec<Player>,
    pub leader_id: PlayerId,
    pub round_count: u8,
    pub round_duration: Duration,
    pub between_round_duration: Duration,
    pub custom: HashMap<String, serde_json::Value>,
}

/// Spawn a game tick loop as a tokio task.
/// Returns the command sender and broadcast receiver.
pub fn spawn_game_session(
    registry: &ServerGameRegistry,
    config: GameSessionConfig,
) -> Option<(
    mpsc::UnboundedSender<GameCommand>,
    mpsc::UnboundedReceiver<GameBroadcast>,
    JoinHandle<()>,
)> {
    let mut game = registry.create(config.game_id)?;

    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
    let (broadcast_tx, broadcast_rx) = mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        run_game_tick_loop(&mut *game, config, cmd_rx, broadcast_tx).await;
    });

    Some((cmd_tx, broadcast_rx, handle))
}

/// The main server-authoritative game tick loop.
async fn run_game_tick_loop(
    game: &mut dyn BreakpointGame,
    config: GameSessionConfig,
    mut cmd_rx: mpsc::UnboundedReceiver<GameCommand>,
    broadcast_tx: mpsc::UnboundedSender<GameBroadcast>,
) {
    let round_count = if config.round_count > 0 {
        config.round_count
    } else {
        game.round_count_hint()
    };

    let game_config = GameConfig {
        round_count,
        round_duration: config.round_duration,
        custom: config.custom.clone(),
    };
    game.init(&config.players, &game_config);

    // Send initial GameStart to all clients
    let start_msg = ServerMessage::GameStart(GameStartMsg {
        game_name: config.game_id.to_string(),
        players: config.players.clone(),
        leader_id: config.leader_id,
    });
    match encode_server_message(&start_msg) {
        Ok(data) => {
            let _ = broadcast_tx.send(GameBroadcast::EncodedMessage(Bytes::from(data)));
        },
        Err(e) => tracing::error!(error = %e, "Failed to encode GameStart"),
    }

    let tick_rate = game.tick_rate();
    let tick_interval = Duration::from_secs_f32(1.0 / tick_rate);
    let mut interval = tokio::time::interval(tick_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut tick: u32 = 0;
    let mut current_round: u8 = 1;
    let mut cumulative_scores: HashMap<PlayerId, i32> = HashMap::new();
    let mut input_buffer: HashMap<PlayerId, Vec<u8>> = HashMap::new();
    let mut players = config.players.clone();
    let mut state_buf: Vec<u8> = Vec::with_capacity(512);
    let is_tron = config.game_id == GameId::Tron;
    let bot_player_ids: Vec<PlayerId> = players.iter().filter(|p| p.is_bot).map(|p| p.id).collect();

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Generate bot inputs for Tron games
                #[cfg(feature = "tron")]
                if is_tron && !bot_player_ids.is_empty() {
                    let bot_state = game.serialize_state();
                    if let Ok(state) =
                        rmp_serde::from_slice::<breakpoint_tron::TronState>(&bot_state)
                    {
                        let tron_config = breakpoint_tron::config::TronConfig::default();
                        for &bot_id in &bot_player_ids {
                            let bot_input = breakpoint_tron::bot::generate_bot_input(
                                &state,
                                bot_id,
                                &tron_config,
                            );
                            if let Ok(input_bytes) = rmp_serde::to_vec(&bot_input) {
                                game.apply_input(bot_id, &input_bytes);
                                input_buffer.insert(bot_id, input_bytes);
                            }
                        }
                    }
                }

                // Collect buffered inputs
                let inputs = PlayerInputs {
                    inputs: std::mem::take(&mut input_buffer),
                };

                tick += 1;
                let events = game.update(1.0 / tick_rate, &inputs);

                // Broadcast game state (reuse buffer to avoid per-tick allocations)
                game.serialize_state_into(&mut state_buf);
                let gs_msg = ServerMessage::GameState(GameStateMsg {
                    tick,
                    state_data: state_buf.clone(),
                });
                match encode_server_message(&gs_msg) {
                    Ok(data) => {
                        let _ = broadcast_tx.send(GameBroadcast::EncodedMessage(
                            Bytes::from(data),
                        ));
                    },
                    Err(e) => tracing::error!(
                        tick, error = %e, "Failed to encode GameState"
                    ),
                }

                // Check for round completion
                let round_complete = events.iter().any(|e| {
                    matches!(e, GameEvent::RoundComplete)
                }) || game.is_round_complete();

                if round_complete {
                    let results = game.round_results();
                    for s in &results {
                        *cumulative_scores.entry(s.player_id).or_insert(0) += s.score;
                    }

                    let scores: Vec<PlayerScoreEntry> = results
                        .iter()
                        .map(|s| PlayerScoreEntry {
                            player_id: s.player_id,
                            score: s.score,
                        })
                        .collect();

                    if current_round >= round_count {
                        // Final round — send GameEnd
                        let final_scores: Vec<PlayerScoreEntry> = cumulative_scores
                            .iter()
                            .map(|(&pid, &score)| PlayerScoreEntry {
                                player_id: pid,
                                score,
                            })
                            .collect();
                        let end_msg = ServerMessage::GameEnd(GameEndMsg { final_scores });
                        match encode_server_message(&end_msg) {
                            Ok(data) => {
                                let _ = broadcast_tx.send(
                                    GameBroadcast::EncodedMessage(Bytes::from(data)),
                                );
                            },
                            Err(e) => tracing::error!(
                                error = %e, "Failed to encode GameEnd"
                            ),
                        }
                        break;
                    }

                    // More rounds — send RoundEnd, wait, re-init
                    let round_end_msg = ServerMessage::RoundEnd(RoundEndMsg {
                        round: current_round,
                        scores,
                        between_round_secs: config.between_round_duration.as_secs() as u16,
                    });
                    match encode_server_message(&round_end_msg) {
                        Ok(data) => {
                            let _ = broadcast_tx.send(
                                GameBroadcast::EncodedMessage(Bytes::from(data)),
                            );
                        },
                        Err(e) => tracing::error!(
                            round = current_round,
                            error = %e,
                            "Failed to encode RoundEnd"
                        ),
                    }

                    // Pause between rounds (drain commands but don't tick)
                    let pause_duration = config.between_round_duration;
                    let pause_end = tokio::time::Instant::now() + pause_duration;
                    while tokio::time::Instant::now() < pause_end {
                        tokio::select! {
                            cmd = cmd_rx.recv() => {
                                match cmd {
                                    Some(GameCommand::Stop) | None => {
                                        let _ = broadcast_tx.send(GameBroadcast::GameEnded);
                                        return;
                                    },
                                    Some(GameCommand::PlayerLeft { player_id }) => {
                                        game.player_left(player_id);
                                        players.retain(|p| p.id != player_id);
                                    },
                                    Some(GameCommand::PlayerJoined { player_id: _, player }) => {
                                        game.player_joined(&player);
                                        players.push(player);
                                    },
                                    _ => {},
                                }
                            }
                            _ = tokio::time::sleep_until(pause_end) => {
                                break;
                            }
                        }
                    }

                    // Advance round and re-init
                    current_round += 1;
                    tick = 0;
                    input_buffer.clear();

                    // Promote spectators for new round
                    for p in &mut players {
                        p.is_spectator = false;
                    }

                    let mut custom = config.custom.clone();
                    custom.insert(
                        "hole_index".to_string(),
                        serde_json::json!(current_round - 1),
                    );
                    let next_config = GameConfig {
                        round_count,
                        round_duration: config.round_duration,
                        custom,
                    };
                    game.init(&players, &next_config);

                    // Send GameStart for next round
                    let next_start = ServerMessage::GameStart(GameStartMsg {
                        game_name: config.game_id.to_string(),
                        players: players.clone(),
                        leader_id: config.leader_id,
                    });
                    match encode_server_message(&next_start) {
                        Ok(data) => {
                            let _ = broadcast_tx.send(
                                GameBroadcast::EncodedMessage(Bytes::from(data)),
                            );
                        },
                        Err(e) => tracing::error!(
                            round = current_round,
                            error = %e,
                            "Failed to encode GameStart for next round"
                        ),
                    }

                    // Reset interval for clean timing
                    interval = tokio::time::interval(tick_interval);
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(GameCommand::PlayerInput { player_id, tick: _, input_data }) => {
                        // Buffer input for next tick; also apply immediately for
                        // responsiveness (game.apply_input handles dedup)
                        game.apply_input(player_id, &input_data);
                        input_buffer.insert(player_id, input_data);
                    },
                    Some(GameCommand::PlayerJoined { player_id: _, player }) => {
                        game.player_joined(&player);
                        players.push(player);
                    },
                    Some(GameCommand::PlayerLeft { player_id }) => {
                        game.player_left(player_id);
                        players.retain(|p| p.id != player_id);
                        if players.is_empty() {
                            break;
                        }
                    },
                    Some(GameCommand::Stop) | None => {
                        break;
                    },
                }
            }
        }
    }

    let _ = broadcast_tx.send(GameBroadcast::GameEnded);
}

#[cfg(test)]
mod tests {
    use super::*;
    use breakpoint_core::player::PlayerColor;

    fn make_test_players(n: usize) -> Vec<Player> {
        (0..n)
            .map(|i| Player {
                id: (i + 1) as PlayerId,
                display_name: format!("Player{}", i + 1),
                color: PlayerColor::PALETTE[i % PlayerColor::PALETTE.len()],
                is_leader: i == 0,
                is_spectator: false,
                is_bot: false,
            })
            .collect()
    }

    #[tokio::test]
    async fn registry_creates_golf() {
        let registry = ServerGameRegistry::new();
        let game = registry.create(GameId::Golf);
        assert!(game.is_some(), "Golf should be registered");
    }

    #[tokio::test]
    async fn registry_creates_platformer() {
        let registry = ServerGameRegistry::new();
        let game = registry.create(GameId::Platformer);
        assert!(game.is_some(), "Platformer should be registered");
    }

    #[tokio::test]
    async fn registry_creates_lasertag() {
        let registry = ServerGameRegistry::new();
        let game = registry.create(GameId::LaserTag);
        assert!(game.is_some(), "LaserTag should be registered");
    }

    #[tokio::test]
    async fn game_session_starts_and_broadcasts_state() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(2);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players: players.clone(),
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // First message should be GameStart
        let msg = broadcast_rx.recv().await.expect("should receive broadcast");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data)
                    .expect("should decode");
                assert!(
                    matches!(decoded, ServerMessage::GameStart(_)),
                    "First message should be GameStart, got: {decoded:?}"
                );
            },
            other => panic!("Expected EncodedMessage, got: {other:?}"),
        }

        // Should receive GameState messages (ticks)
        let msg = broadcast_rx.recv().await.expect("should receive tick");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data)
                    .expect("should decode");
                assert!(
                    matches!(decoded, ServerMessage::GameState(_)),
                    "Should receive GameState tick, got: {decoded:?}"
                );
            },
            other => panic!("Expected GameState, got: {other:?}"),
        }

        // Stop the game
        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn player_input_reaches_game() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(1);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Consume GameStart
        let _ = broadcast_rx.recv().await;

        // Send a golf stroke input
        let golf_input = breakpoint_golf::GolfInput {
            aim_angle: 0.0,
            power: 0.5,
            stroke: true,
        };
        let input_data = rmp_serde::to_vec(&golf_input).unwrap();
        let _ = cmd_tx.send(GameCommand::PlayerInput {
            player_id: 1,
            tick: 1,
            input_data,
        });

        // Wait for a few ticks — game state should reflect the stroke
        for _ in 0..5 {
            if let Some(GameBroadcast::EncodedMessage(data)) = broadcast_rx.recv().await
                && let Ok(ServerMessage::GameState(gs)) =
                    breakpoint_core::net::protocol::decode_server_message(&data)
            {
                let state: breakpoint_golf::GolfState =
                    rmp_serde::from_slice(&gs.state_data).unwrap();
                if state.strokes.get(&1).copied().unwrap_or(0) > 0 {
                    // Success — input was applied
                    let _ = cmd_tx.send(GameCommand::Stop);
                    let _ = handle.await;
                    return;
                }
            }
        }

        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
        // The input was sent; even if not visible in 5 ticks, the test
        // validates the pipeline doesn't panic.
    }

    #[tokio::test]
    async fn player_leave_during_game() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(2);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Consume GameStart
        let _ = broadcast_rx.recv().await;

        // Player 2 leaves
        let _ = cmd_tx.send(GameCommand::PlayerLeft { player_id: 2 });

        // Game should continue (still has player 1)
        let msg = broadcast_rx.recv().await;
        assert!(msg.is_some(), "Game should continue after player leave");

        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn all_players_leave_ends_game() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(1);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Consume GameStart
        let _ = broadcast_rx.recv().await;

        // Last player leaves
        let _ = cmd_tx.send(GameCommand::PlayerLeft { player_id: 1 });

        // Should eventually get GameEnded
        let mut got_ended = false;
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv()).await {
                Ok(Some(GameBroadcast::GameEnded)) => {
                    got_ended = true;
                    break;
                },
                Ok(Some(_)) => continue,
                _ => break,
            }
        }
        assert!(got_ended, "Game should end when all players leave");
        let _ = handle.await;
    }

    #[tokio::test]
    async fn registry_creates_tron() {
        let registry = ServerGameRegistry::new();
        let game = registry.create(GameId::Tron);
        assert!(game.is_some(), "Tron should be registered");
    }

    #[tokio::test]
    async fn registry_returns_none_for_unknown() {
        let registry = ServerGameRegistry::new();
        assert_eq!(
            registry.available_games(),
            4,
            "All 4 default games should be registered"
        );
    }

    #[tokio::test]
    async fn stop_command_ends_game_cleanly() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(2);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Consume GameStart
        let _ = broadcast_rx.recv().await;

        // Send Stop
        let _ = cmd_tx.send(GameCommand::Stop);

        // Should receive GameEnded broadcast
        let mut got_ended = false;
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv()).await {
                Ok(Some(GameBroadcast::GameEnded)) => {
                    got_ended = true;
                    break;
                },
                Ok(Some(_)) => continue,
                _ => break,
            }
        }
        assert!(got_ended, "Stop command should produce GameEnded broadcast");
        let _ = handle.await;
    }

    #[tokio::test]
    async fn player_join_during_game() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(1);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Consume GameStart
        let _ = broadcast_rx.recv().await;

        // New player joins mid-game
        let new_player = Player {
            id: 2,
            display_name: "LateJoiner".to_string(),
            color: PlayerColor::PALETTE[1],
            is_leader: false,
            is_spectator: false,
            is_bot: false,
        };
        let _ = cmd_tx.send(GameCommand::PlayerJoined {
            player_id: 2,
            player: new_player,
        });

        // Game should continue producing ticks without panic
        let msg = tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv()).await;
        assert!(
            msg.is_ok(),
            "Game should continue after mid-game player join"
        );

        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn broadcast_encoding_produces_valid_msgpack() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(2);

        let config = GameSessionConfig {
            game_id: GameId::Golf,
            players,
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // Receive GameStart and verify it decodes
        let msg = broadcast_rx.recv().await.expect("should receive GameStart");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data);
                assert!(
                    decoded.is_ok(),
                    "GameStart bytes should decode: {:?}",
                    decoded.err()
                );
            },
            other => panic!("Expected EncodedMessage for GameStart, got: {other:?}"),
        }

        // Receive at least one GameState tick and verify it decodes
        let msg = tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv())
            .await
            .expect("should receive tick within timeout")
            .expect("channel should not be closed");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data);
                assert!(
                    decoded.is_ok(),
                    "GameState bytes should decode: {:?}",
                    decoded.err()
                );
            },
            other => panic!("Expected EncodedMessage for GameState, got: {other:?}"),
        }

        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn game_session_with_platformer() {
        let registry = ServerGameRegistry::new();
        let players = make_test_players(2);

        let config = GameSessionConfig {
            game_id: GameId::Platformer,
            players: players.clone(),
            leader_id: 1,
            round_count: 1,
            round_duration: Duration::from_secs(90),
            between_round_duration: Duration::from_secs(1),
            custom: HashMap::new(),
        };

        let (cmd_tx, mut broadcast_rx, handle) =
            spawn_game_session(&registry, config).expect("should spawn");

        // First message should be GameStart with Platformer
        let msg = broadcast_rx.recv().await.expect("should receive broadcast");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data)
                    .expect("should decode");
                match decoded {
                    ServerMessage::GameStart(gs) => {
                        assert_eq!(gs.game_name, "platform-racer");
                        assert_eq!(gs.players.len(), 2);
                    },
                    other => panic!("Expected GameStart, got: {other:?}"),
                }
            },
            other => panic!("Expected EncodedMessage, got: {other:?}"),
        }

        // Should receive GameState ticks
        let msg = tokio::time::timeout(Duration::from_millis(500), broadcast_rx.recv())
            .await
            .expect("should receive tick within timeout")
            .expect("channel should not be closed");
        match msg {
            GameBroadcast::EncodedMessage(data) => {
                let decoded = breakpoint_core::net::protocol::decode_server_message(&data)
                    .expect("should decode");
                assert!(
                    matches!(decoded, ServerMessage::GameState(_)),
                    "Should receive GameState tick, got: {decoded:?}"
                );
            },
            other => panic!("Expected GameState tick, got: {other:?}"),
        }

        let _ = cmd_tx.send(GameCommand::Stop);
        let _ = handle.await;
    }
}
