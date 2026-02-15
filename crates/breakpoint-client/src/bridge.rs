use crate::app::App;

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
                })
            }),
            "connected": app.ws.is_connected(),
            "muted": app.audio_settings.muted,
        });

        if let Ok(json_str) = serde_json::to_string(&state) {
            let _ = js_sys::eval(&format!(
                "window._breakpointUpdate && window._breakpointUpdate({})",
                json_str
            ));
        }
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = app;
}

/// Show disconnect banner via JS.
pub fn show_disconnect_banner() {
    #[cfg(target_family = "wasm")]
    {
        let _ = js_sys::eval("window._breakpointDisconnect && window._breakpointDisconnect()");
    }
}

/// Hide disconnect banner via JS.
pub fn hide_disconnect_banner() {
    #[cfg(target_family = "wasm")]
    {
        let _ = js_sys::eval("window._breakpointReconnect && window._breakpointReconnect()");
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
            });
            if let Ok(data) = encode_client_message(&msg) {
                let _ = app.ws.send(&data);
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
            });
            if let Ok(data) = encode_client_message(&msg) {
                let _ = app.ws.send(&data);
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
                });
                if let Ok(data) = encode_client_message(&msg) {
                    let _ = app.ws.send(&data);
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
}

#[cfg(not(target_family = "wasm"))]
#[allow(dead_code)]
pub fn attach_ui_callbacks(_app: &std::rc::Rc<std::cell::RefCell<App>>) {}
