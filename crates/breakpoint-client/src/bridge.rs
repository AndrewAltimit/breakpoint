use crate::app::App;

#[cfg(target_family = "wasm")]
use wasm_bindgen::JsCast;

/// Push UI state to JavaScript each frame.
pub fn push_ui_state(app: &App) {
    #[cfg(target_family = "wasm")]
    {
        let state = serde_json::json!({
            "appState": format!("{:?}", app.state),
            "lobby": {
                "playerName": app.lobby.player_name,
                "roomCode": app.lobby.room_code,
                "connected": app.lobby.connected,
                "isLeader": app.lobby.is_leader,
                "isSpectator": app.lobby.is_spectator,
                "selectedGame": app.lobby.selected_game.to_string(),
                "joinCodeInput": app.lobby.join_code_input,
                "statusMessage": app.lobby.status_message,
                "errorMessage": app.lobby.error_message,
                "players": app.lobby.players.iter().map(|p| {
                    serde_json::json!({
                        "id": p.id,
                        "name": p.display_name,
                        "isLeader": p.is_leader,
                        "isBot": p.is_bot,
                    })
                }).collect::<Vec<_>>(),
            },
            "overlay": {
                "tickerText": app.overlay.ticker.display_text(),
                "unreadCount": app.overlay.unread_count,
                "dashboardVisible": app.overlay.dashboard_visible,
                "pendingActions": app.overlay.toasts.pending_count(),
                "toasts": app.overlay.toasts.visible().iter().map(|t| {
                    serde_json::json!({
                        "id": t.event.id,
                        "title": t.event.title,
                        "source": t.event.source,
                        "actor": t.event.actor,
                        "priority": format!("{:?}", t.event.priority),
                        "claimedBy": t.claimed_by,
                    })
                }).collect::<Vec<_>>(),
            },
            "game": app.game.as_ref().map(|g| {
                serde_json::json!({
                    "gameId": g.game_id.to_string(),
                    "tick": g.tick,
                })
            }),
            "roundTracker": app.round_tracker.as_ref().map(|rt| {
                serde_json::json!({
                    "currentRound": rt.current_round,
                    "totalRounds": rt.total_rounds,
                    "scores": rt.cumulative_scores,
                    "roundScoresHistory": rt.round_scores,
                })
            }),
            "connected": app.ws.is_connected(),
            "muted": app.audio_settings.muted,
            "golfHud": build_golf_hud(app),
            "platformerHud": build_platformer_hud(app),
            "lasertagHud": build_lasertag_hud(app),
            "tronHud": build_tron_hud(app),
            "betweenRoundCountdown": app.between_round_end_time.map(|end| {
                let remaining = (end - app.prev_timestamp) / 1000.0;
                if remaining > 0.0 { remaining } else { 0.0 }
            }),
            "gameOverCountdown": app.game_over_timestamp.map(|start| {
                let elapsed = (app.prev_timestamp - start) / 1000.0;
                let remaining = 30.0 - elapsed;
                if remaining > 0.0 { remaining } else { 0.0 }
            }),
        });

        match serde_json::to_string(&state) {
            Ok(json_str) => {
                call_window_fn("_breakpointUpdate", Some(&json_str));
            },
            Err(e) => {
                crate::diag::console_warn!("Failed to serialize UI state: {e}");
            },
        }
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = app;
}

/// Build Golf HUD data (hole/par/strokes/sunk indicators).
#[cfg(target_family = "wasm")]
fn build_golf_hud(app: &App) -> serde_json::Value {
    use breakpoint_core::game_trait::GameId;

    use crate::app::AppState;

    if app.state != AppState::InGame {
        return serde_json::Value::Null;
    }
    let Some(ref active) = app.game else {
        return serde_json::Value::Null;
    };
    if active.game_id != GameId::Golf {
        return serde_json::Value::Null;
    }

    let state: Option<breakpoint_golf::GolfState> = crate::game::read_game_state(active);
    let Some(state) = state else {
        return serde_json::Value::Null;
    };

    let courses = breakpoint_golf::course::all_courses();
    let course = courses.get(state.course_index as usize);
    let par = course.map(|c| c.par).unwrap_or(3);
    let hole_name = course.map(|c| c.name.as_str()).unwrap_or("Hole");

    let players_json: Vec<serde_json::Value> = app
        .lobby
        .players
        .iter()
        .map(|p| {
            let strokes = state.strokes.get(&p.id).copied().unwrap_or(0);
            let is_sunk = state.balls.get(&p.id).map(|b| b.is_sunk).unwrap_or(false);
            let sunk_rank = state
                .sunk_order
                .iter()
                .position(|&id| id == p.id)
                .map(|i| i + 1);
            serde_json::json!({
                "id": p.id,
                "name": p.display_name,
                "strokes": strokes,
                "isSunk": is_sunk,
                "sunkRank": sunk_rank,
            })
        })
        .collect();

    serde_json::json!({
        "holeIndex": state.course_index,
        "holeName": hole_name,
        "par": par,
        "players": players_json,
        "roundTimer": state.round_timer,
    })
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
fn build_golf_hud(_app: &App) -> serde_json::Value {
    serde_json::Value::Null
}

/// Build Platformer HUD data (rankings, mode, hazard, eliminations).
#[cfg(target_family = "wasm")]
fn build_platformer_hud(app: &App) -> serde_json::Value {
    use breakpoint_core::game_trait::GameId;

    use crate::app::AppState;

    if app.state != AppState::InGame {
        return serde_json::Value::Null;
    }
    let Some(ref active) = app.game else {
        return serde_json::Value::Null;
    };
    if active.game_id != GameId::Platformer {
        return serde_json::Value::Null;
    }

    let state: Option<breakpoint_platformer::PlatformerState> =
        crate::game::read_game_state(active);
    let Some(state) = state else {
        return serde_json::Value::Null;
    };

    let mode_str = match state.mode {
        breakpoint_platformer::GameMode::Race => "Race",
        breakpoint_platformer::GameMode::Survival => "Survival",
    };

    let players_json: Vec<serde_json::Value> = app
        .lobby
        .players
        .iter()
        .map(|p| {
            let ps = state.players.get(&p.id);
            let eliminated = ps.map(|s| s.eliminated).unwrap_or(false);
            let finished = ps.map(|s| s.finished).unwrap_or(false);
            let finish_rank = state
                .finish_order
                .iter()
                .position(|&id| id == p.id)
                .map(|i| i + 1);
            serde_json::json!({
                "id": p.id,
                "name": p.display_name,
                "eliminated": eliminated,
                "finished": finished,
                "finishRank": finish_rank,
            })
        })
        .collect();

    serde_json::json!({
        "mode": mode_str,
        "players": players_json,
        "hazardY": state.hazard_y,
        "eliminationCount": state.elimination_order.len(),
        "finishCount": state.finish_order.len(),
        "roundTimer": state.round_timer,
    })
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
fn build_platformer_hud(_app: &App) -> serde_json::Value {
    serde_json::Value::Null
}

/// Build LaserTag HUD data (scores, team scores, power-ups, stun).
#[cfg(target_family = "wasm")]
fn build_lasertag_hud(app: &App) -> serde_json::Value {
    use breakpoint_core::game_trait::GameId;

    use crate::app::AppState;

    if app.state != AppState::InGame {
        return serde_json::Value::Null;
    }
    let Some(ref active) = app.game else {
        return serde_json::Value::Null;
    };
    if active.game_id != GameId::LaserTag {
        return serde_json::Value::Null;
    }

    let state: Option<breakpoint_lasertag::LaserTagState> = crate::game::read_game_state(active);
    let Some(state) = state else {
        return serde_json::Value::Null;
    };

    let local_id = app.network_role.as_ref().map(|r| r.local_player_id);

    let team_mode_str = match &state.team_mode {
        breakpoint_lasertag::TeamMode::FreeForAll => "FFA".to_string(),
        breakpoint_lasertag::TeamMode::Teams { team_count } => {
            format!("{team_count} Teams")
        },
    };

    let players_json: Vec<serde_json::Value> = app
        .lobby
        .players
        .iter()
        .map(|p| {
            let tags = state.tags_scored.get(&p.id).copied().unwrap_or(0);
            let ps = state.players.get(&p.id);
            let stunned = ps.map(|s| s.stun_remaining > 0.0).unwrap_or(false);
            let team = state.teams.get(&p.id).copied();
            let is_local = local_id == Some(p.id);
            serde_json::json!({
                "id": p.id,
                "name": p.display_name,
                "tags": tags,
                "stunned": stunned,
                "team": team,
                "isLocal": is_local,
            })
        })
        .collect();

    // Team scores
    let mut team_scores: std::collections::HashMap<u8, u32> = std::collections::HashMap::new();
    if matches!(state.team_mode, breakpoint_lasertag::TeamMode::Teams { .. }) {
        for (&pid, &tags) in &state.tags_scored {
            if let Some(&team) = state.teams.get(&pid) {
                *team_scores.entry(team).or_insert(0) += tags;
            }
        }
    }

    let local_stun = local_id
        .and_then(|id| state.players.get(&id))
        .map(|s| s.stun_remaining)
        .unwrap_or(0.0);

    serde_json::json!({
        "teamMode": team_mode_str,
        "players": players_json,
        "teamScores": team_scores,
        "localStunRemaining": local_stun,
        "roundTimer": state.round_timer,
    })
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
fn build_lasertag_hud(_app: &App) -> serde_json::Value {
    serde_json::Value::Null
}

/// Build Tron HUD data (player name positions, minimap walls, gauges).
#[cfg(target_family = "wasm")]
fn build_tron_hud(app: &App) -> serde_json::Value {
    {
        use breakpoint_core::game_trait::GameId;

        use crate::app::AppState;

        // Only active during Tron InGame
        if app.state != AppState::InGame {
            return serde_json::Value::Null;
        }
        let Some(ref active) = app.game else {
            return serde_json::Value::Null;
        };
        if active.game_id != GameId::Tron {
            return serde_json::Value::Null;
        }

        let state: Option<breakpoint_tron::TronState> = crate::game::read_game_state(active);
        let Some(state) = state else {
            return serde_json::Value::Null;
        };

        let local_id = app.network_role.as_ref().map(|r| r.local_player_id);
        let vp = app.camera.view_projection();

        // Player colors (same order as tron_render)
        const PLAYER_COLORS_HEX: [&str; 8] = [
            "#00d9ff", "#ffcc00", "#1aff33", "#ff0099", "#9933ff", "#ff5900", "#00ffb3", "#ff1a1a",
        ];

        // Build player index for color mapping
        let mut player_index: std::collections::HashMap<u64, usize> =
            std::collections::HashMap::new();
        for (i, (&pid, _)) in state.players.iter().enumerate() {
            player_index.insert(pid, i);
        }

        // Player name labels with screen positions
        let mut players_json = Vec::new();
        for (&pid, cycle) in &state.players {
            let color_idx = player_index.get(&pid).copied().unwrap_or(0) % 8;
            let color_hex = PLAYER_COLORS_HEX[color_idx];

            // Find display name from lobby players
            let name = app
                .lobby
                .players
                .iter()
                .find(|p| p.id == pid)
                .map(|p| p.display_name.as_str())
                .unwrap_or("Player");

            let is_local = local_id == Some(pid);

            // Project cycle position to screen (above the cycle)
            let world_pos = glam::Vec3::new(cycle.x, 3.0, cycle.z);
            let (screen_x, screen_y) = app
                .renderer
                .world_to_screen(world_pos, &vp)
                .unwrap_or((-999.0, -999.0));

            players_json.push(serde_json::json!({
                "name": name,
                "screenX": screen_x,
                "screenY": screen_y,
                "color": color_hex,
                "alive": cycle.alive,
                "speed": cycle.speed,
                "rubber": cycle.rubber,
                "brakeFuel": cycle.brake_fuel,
                "isLocal": is_local,
            }));
        }

        // Minimap data — wall segments + cycle positions (compact)
        let minimap_walls: Vec<serde_json::Value> = state
            .wall_segments
            .iter()
            .map(|w| {
                let cidx = player_index.get(&w.owner_id).copied().unwrap_or(0) % 8;
                serde_json::json!([w.x1, w.z1, w.x2, w.z2, cidx])
            })
            .collect();

        let minimap_cycles: Vec<serde_json::Value> = state
            .players
            .iter()
            .map(|(&pid, c)| {
                let cidx = player_index.get(&pid).copied().unwrap_or(0) % 8;
                serde_json::json!([c.x, c.z, cidx, c.alive])
            })
            .collect();

        serde_json::json!({
            "players": players_json,
            "arenaWidth": state.arena_width,
            "arenaDepth": state.arena_depth,
            "minimapWalls": minimap_walls,
            "minimapCycles": minimap_cycles,
        })
    }
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
fn build_tron_hud(_app: &App) -> serde_json::Value {
    serde_json::Value::Null
}

/// Show disconnect banner via JS.
pub fn show_disconnect_banner() {
    #[cfg(target_family = "wasm")]
    call_window_fn("_breakpointDisconnect", None);
}

/// Hide disconnect banner via JS.
pub fn hide_disconnect_banner() {
    #[cfg(target_family = "wasm")]
    call_window_fn("_breakpointReconnect", None);
}

/// Call a function on the window object without eval().
/// If `json_arg` is Some, the JSON string is parsed to a JS object and passed as the argument.
#[cfg(target_family = "wasm")]
fn call_window_fn(name: &str, json_arg: Option<&str>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Ok(val) = js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str(name)) else {
        return;
    };
    if !val.is_function() {
        return;
    }
    let func: js_sys::Function = val.unchecked_into();
    let result = if let Some(json_str) = json_arg {
        match js_sys::JSON::parse(json_str) {
            Ok(parsed) => func.call1(&wasm_bindgen::JsValue::NULL, &parsed),
            Err(e) => {
                crate::diag::console_warn!("JSON parse failed for {name}: {e:?}");
                return;
            },
        }
    } else {
        func.call0(&wasm_bindgen::JsValue::NULL)
    };
    if let Err(e) = result {
        crate::diag::console_warn!("JS bridge {name} failed: {e:?}");
    }
}

/// Attach keyboard and mouse event listeners to the canvas and document.
#[cfg(target_family = "wasm")]
pub fn attach_input_listeners(app: &std::rc::Rc<std::cell::RefCell<App>>) {
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    use crate::input::MouseButton;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let document = match window.document() {
        Some(d) => d,
        None => return,
    };

    // Keyboard: keydown
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |evt: web_sys::KeyboardEvent| {
                let code = evt.code();
                // Prevent default for game keys
                if matches!(
                    code.as_str(),
                    "Space" | "Tab" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight"
                ) {
                    evt.prevent_default();
                }
                app.borrow_mut().input.on_key_down(code);
            },
        );
        let _ =
            document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // Keyboard: keyup
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(
            move |evt: web_sys::KeyboardEvent| {
                app.borrow_mut().input.on_key_up(evt.code());
            },
        );
        let _ =
            document.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // Mouse events on canvas
    let canvas = document.get_element_by_id("game-canvas");
    let Some(canvas) = canvas else {
        return;
    };

    // mousedown
    {
        let app = Rc::clone(app);
        let closure =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
                let button = match evt.button() {
                    0 => MouseButton::Left,
                    1 => MouseButton::Middle,
                    2 => MouseButton::Right,
                    _ => return,
                };
                app.borrow_mut().input.on_mouse_down(button);
            });
        let _ =
            canvas.add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // mouseup
    {
        let app = Rc::clone(app);
        let closure =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
                let button = match evt.button() {
                    0 => MouseButton::Left,
                    1 => MouseButton::Middle,
                    2 => MouseButton::Right,
                    _ => return,
                };
                app.borrow_mut().input.on_mouse_up(button);
            });
        let _ =
            canvas.add_event_listener_with_callback("mouseup", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // mousemove
    {
        let app = Rc::clone(app);
        let closure =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
                app.borrow_mut()
                    .input
                    .on_mouse_move(evt.offset_x() as f32, evt.offset_y() as f32);
            });
        let _ =
            canvas.add_event_listener_with_callback("mousemove", closure.as_ref().unchecked_ref());
        closure.forget();
    }

    // Prevent context menu on canvas
    {
        let closure =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |evt: web_sys::MouseEvent| {
                evt.prevent_default();
            });
        let _ = canvas
            .add_event_listener_with_callback("contextmenu", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
pub fn attach_input_listeners(_app: &std::rc::Rc<std::cell::RefCell<App>>) {}

/// Attach JS->Rust bridge callbacks via global functions on window.
#[cfg(target_family = "wasm")]
pub fn attach_ui_callbacks(app: &std::rc::Rc<std::cell::RefCell<App>>) {
    use std::rc::Rc;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    use breakpoint_core::game_trait::GameId;
    use breakpoint_core::net::messages::{ClientMessage, JoinRoomMsg, RequestGameStartMsg};
    use breakpoint_core::net::protocol::{PROTOCOL_VERSION, encode_client_message};
    use breakpoint_core::player::PlayerColor;

    use crate::app::AppState;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };

    // ui_set_player_name(name)
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(String)>::new(move |name: String| {
            let mut app = app.borrow_mut();
            let name = name.trim().to_string();
            if !name.is_empty() {
                app.lobby.player_name = name;
            }
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpSetPlayerName".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_create_room
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let mut app = app.borrow_mut();
            if app.lobby.connected {
                app.lobby.status_message =
                    Some("Already in a room. Refresh to create a new one.".to_string());
                return;
            }
            if !app.ws.has_connection() {
                let url = app.lobby.ws_url.clone();
                if let Err(e) = app.ws.connect(&url) {
                    app.lobby.status_message = Some(format!("Connection failed: {e}"));
                    return;
                }
            }
            app.lobby.is_leader = true;
            let color = PlayerColor::PALETTE[app.lobby.color_index % PlayerColor::PALETTE.len()];
            let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                room_code: String::new(),
                player_name: app.lobby.player_name.clone(),
                player_color: color,
                protocol_version: PROTOCOL_VERSION,
                session_token: None,
            });
            match encode_client_message(&msg) {
                Ok(data) => {
                    if let Err(e) = app.ws.send(&data) {
                        crate::diag::console_warn!("Failed to send JoinRoom (create): {e}");
                    }
                },
                Err(e) => crate::diag::console_warn!("Failed to encode JoinRoom (create): {e}"),
            }
            app.lobby.status_message = Some("Creating room...".to_string());
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpCreateRoom".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_join_room(code)
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(String)>::new(move |code: String| {
            let mut app = app.borrow_mut();
            if app.lobby.connected {
                app.lobby.status_message =
                    Some("Already in a room. Refresh to join a new one.".to_string());
                return;
            }
            let code = code.trim().to_uppercase();
            if code.is_empty() {
                app.lobby.status_message =
                    Some("Type a room code first (e.g. ABCD-1234)".to_string());
                return;
            }
            if !app.ws.has_connection() {
                let url = app.lobby.ws_url.clone();
                if let Err(e) = app.ws.connect(&url) {
                    app.lobby.status_message = Some(format!("Connection failed: {e}"));
                    return;
                }
            }
            app.lobby.is_leader = false;
            let color = PlayerColor::PALETTE[app.lobby.color_index % PlayerColor::PALETTE.len()];
            let msg = ClientMessage::JoinRoom(JoinRoomMsg {
                room_code: code.clone(),
                player_name: app.lobby.player_name.clone(),
                player_color: color,
                protocol_version: PROTOCOL_VERSION,
                session_token: None,
            });
            match encode_client_message(&msg) {
                Ok(data) => {
                    if let Err(e) = app.ws.send(&data) {
                        crate::diag::console_warn!("Failed to send JoinRoom (join): {e}");
                    }
                },
                Err(e) => crate::diag::console_warn!("Failed to encode JoinRoom (join): {e}"),
            }
            app.lobby.status_message = Some(format!("Joining room {code}..."));
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpJoinRoom".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_start_game
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let app = app.borrow();
            if app.lobby.is_leader {
                let msg = ClientMessage::RequestGameStart(RequestGameStartMsg {
                    game_name: app.lobby.selected_game.to_string(),
                    custom: app.lobby.game_settings.clone(),
                });
                match encode_client_message(&msg) {
                    Ok(data) => {
                        if let Err(e) = app.ws.send(&data) {
                            crate::diag::console_warn!("Failed to send RequestGameStart: {e}");
                        }
                    },
                    Err(e) => crate::diag::console_warn!("Failed to encode RequestGameStart: {e}"),
                }
            }
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpStartGame".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_select_game(name)
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(String)>::new(move |name: String| {
            let mut app = app.borrow_mut();
            app.lobby.selected_game = GameId::from_str_opt(&name).unwrap_or_default();
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpSelectGame".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_set_game_setting(key, value_json)
    {
        let app = Rc::clone(app);
        let closure =
            Closure::<dyn FnMut(String, String)>::new(move |key: String, value_json: String| {
                let mut app = app.borrow_mut();
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&value_json) {
                    app.lobby.game_settings.insert(key, val);
                }
            });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpSetGameSetting".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_claim_alert(event_id)
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(String)>::new(move |event_id: String| {
            let app = app.borrow();
            app.overlay.claim_alert(&event_id, &app.ws);
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpClaimAlert".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_toggle_mute
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let mut app = app.borrow_mut();
            app.audio_settings.muted = !app.audio_settings.muted;
            crate::storage::with_local_storage(|storage| {
                let _ = storage.set_item(
                    "audio_muted",
                    if app.audio_settings.muted {
                        "true"
                    } else {
                        "false"
                    },
                );
            });
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpToggleMute".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_return_to_lobby
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let mut app = app.borrow_mut();
            app.reconnect_info = None;
            // Keep WebSocket alive — server resets room to Lobby automatically
            // after the game ends (via end_game_session in broadcast task).
            app.transition_to(AppState::Lobby);
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpReturnToLobby".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_toggle_dashboard
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let mut app = app.borrow_mut();
            app.overlay.dashboard_visible = !app.overlay.dashboard_visible;
            if app.overlay.dashboard_visible {
                app.overlay.unread_count = 0;
            }
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpToggleDashboard".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_add_bot
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut()>::new(move || {
            let app = app.borrow();
            let msg = ClientMessage::AddBot(breakpoint_core::net::messages::AddBotMsg {});
            match encode_client_message(&msg) {
                Ok(data) => {
                    if let Err(e) = app.ws.send(&data) {
                        crate::diag::console_warn!("Failed to send AddBot: {e}");
                    }
                },
                Err(e) => crate::diag::console_warn!("Failed to encode AddBot: {e}"),
            }
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpAddBot".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }

    // ui_remove_bot(player_id)
    {
        let app = Rc::clone(app);
        let closure = Closure::<dyn FnMut(f64)>::new(move |player_id: f64| {
            let app = app.borrow();
            let msg = ClientMessage::RemoveBot(breakpoint_core::net::messages::RemoveBotMsg {
                player_id: player_id as u64,
            });
            match encode_client_message(&msg) {
                Ok(data) => {
                    if let Err(e) = app.ws.send(&data) {
                        crate::diag::console_warn!("Failed to send RemoveBot: {e}");
                    }
                },
                Err(e) => crate::diag::console_warn!("Failed to encode RemoveBot: {e}"),
            }
        });
        let _ = js_sys::Reflect::set(
            &window,
            &"_bpRemoveBot".into(),
            closure.as_ref().unchecked_ref(),
        );
        closure.forget();
    }
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
pub fn attach_ui_callbacks(_app: &std::rc::Rc<std::cell::RefCell<App>>) {}
