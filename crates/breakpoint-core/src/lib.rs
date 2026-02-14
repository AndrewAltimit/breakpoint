pub mod events;
pub mod game_registry;
pub mod game_trait;
pub mod net;
pub mod overlay;
pub mod player;
pub mod powerup;
pub mod room;
pub mod time;

#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    use std::collections::HashMap;
    use std::time::Duration;

    use crate::events::{Event, EventType, Priority};
    use crate::game_trait::{
        BreakpointGame, GameConfig, GameEvent, PlayerId, PlayerInputs, PlayerScore,
    };
    use crate::player::{Player, PlayerColor};

    /// Create `n` test players with sequential IDs starting at 1.
    pub fn make_players(n: usize) -> Vec<Player> {
        (0..n)
            .map(|i| Player {
                id: i as PlayerId + 1,
                display_name: format!("Player{}", i + 1),
                color: PlayerColor::default(),
                is_host: i == 0,
                is_spectator: false,
            })
            .collect()
    }

    /// Create a default GameConfig with the given round duration in seconds.
    pub fn default_config(round_duration_secs: u64) -> GameConfig {
        GameConfig {
            round_count: 1,
            round_duration: Duration::from_secs(round_duration_secs),
            custom: HashMap::new(),
        }
    }

    /// Create a generic test event with the given id.
    pub fn make_test_event(id: &str) -> Event {
        Event {
            id: id.to_string(),
            event_type: EventType::PrOpened,
            source: "test".to_string(),
            priority: Priority::Notice,
            title: format!("Test event {id}"),
            body: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            url: None,
            actor: None,
            tags: vec![],
            action_required: false,
            group_key: None,
            expires_at: None,
            metadata: HashMap::new(),
        }
    }

    /// Run N game ticks with empty inputs, returning all accumulated events.
    pub fn run_game_ticks(game: &mut dyn BreakpointGame, n: usize, dt: f32) -> Vec<GameEvent> {
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        let mut all_events = Vec::new();
        for _ in 0..n {
            all_events.extend(game.update(dt, &empty));
        }
        all_events
    }

    /// Assert that the game's serialized state differs from `before`.
    pub fn assert_game_state_changed(game: &dyn BreakpointGame, before: &[u8]) {
        let after = game.serialize_state();
        assert_ne!(
            before,
            &after[..],
            "Game state should have changed after operation"
        );
    }

    // ================================================================
    // Game Trait Contract Tests
    // ================================================================
    // These functions form a generic test suite that every BreakpointGame
    // implementation must pass. Game crates call them from their own
    // #[cfg(test)] modules with a concrete game instance and valid input.

    /// After init() with N players, serialize_state() must return non-empty bytes.
    pub fn contract_init_creates_player_state(game: &mut dyn BreakpointGame, player_count: usize) {
        let players = make_players(player_count);
        let config = default_config(90);
        game.init(&players, &config);
        let state = game.serialize_state();
        assert!(
            !state.is_empty(),
            "serialize_state() must return non-empty bytes after init"
        );
    }

    /// apply_input() with valid data followed by update() must change state.
    pub fn contract_apply_input_changes_state(
        game: &mut dyn BreakpointGame,
        valid_input: &[u8],
        player_id: PlayerId,
    ) {
        let before = game.serialize_state();
        game.apply_input(player_id, valid_input);
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(0.1, &empty);
        let after = game.serialize_state();
        assert_ne!(
            before, after,
            "State must change after apply_input + update"
        );
    }

    /// update() with dt>0 must advance the round timer.
    pub fn contract_update_advances_time(game: &mut dyn BreakpointGame) {
        let before = game.serialize_state();
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(1.0, &empty);
        let after = game.serialize_state();
        assert_ne!(
            before, after,
            "update(dt>0) must advance game state (timer)"
        );
    }

    /// Running update() enough times must eventually reach is_round_complete().
    pub fn contract_round_eventually_completes(game: &mut dyn BreakpointGame, max_ticks: usize) {
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        for _ in 0..max_ticks {
            game.update(1.0, &empty);
            if game.is_round_complete() {
                return;
            }
        }
        assert!(
            game.is_round_complete(),
            "Game must complete after {max_ticks} ticks of 1s each"
        );
    }

    /// serialize_state → apply_state roundtrip: the game must produce
    /// equivalent state after applying its own serialized output.
    /// We verify by doing serialize→apply→serialize→apply→serialize and
    /// checking the last two serializations are identical (stable after
    /// one roundtrip), which handles HashMap iteration order differences.
    pub fn contract_state_roundtrip_preserves(game: &mut dyn BreakpointGame) {
        let state_a = game.serialize_state();
        game.apply_state(&state_a);
        let state_b = game.serialize_state();
        game.apply_state(&state_b);
        let state_c = game.serialize_state();
        assert_eq!(
            state_b, state_c,
            "State must be stable after serialize→apply→serialize roundtrip"
        );
    }

    /// pause() must freeze timer, resume() must unfreeze it.
    pub fn contract_pause_stops_updates(game: &mut dyn BreakpointGame) {
        game.pause();
        let before = game.serialize_state();
        let empty = PlayerInputs {
            inputs: HashMap::new(),
        };
        game.update(1.0, &empty);
        let during_pause = game.serialize_state();
        assert_eq!(before, during_pause, "State must not change while paused");

        game.resume();
        game.update(1.0, &empty);
        let after_resume = game.serialize_state();
        assert_ne!(during_pause, after_resume, "State must change after resume");
    }

    /// player_left() must remove player data from state.
    pub fn contract_player_left_cleanup(
        game: &mut dyn BreakpointGame,
        player_id: PlayerId,
        player_count: usize,
    ) {
        let before = game.serialize_state();
        game.player_left(player_id);
        let after = game.serialize_state();
        assert_ne!(before, after, "player_left must change state");
        // After removing a player, round_results should have one fewer entry
        let results = game.round_results();
        assert_eq!(
            results.len(),
            player_count - 1,
            "round_results should have {} entries after removing player",
            player_count - 1
        );
    }

    /// round_results() must return an entry for each active player.
    pub fn contract_round_results_complete(
        game: &dyn BreakpointGame,
        expected_players: usize,
    ) -> Vec<PlayerScore> {
        let results = game.round_results();
        assert_eq!(
            results.len(),
            expected_players,
            "round_results must have one entry per active player"
        );
        results
    }
}
