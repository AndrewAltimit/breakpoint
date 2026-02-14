use std::cell::RefCell;
use std::rc::Rc;

use bevy::prelude::*;

#[cfg(target_family = "wasm")]
use wasm_bindgen::prelude::*;

/// Buffer for messages received from the WebSocket.
#[derive(Default)]
struct MessageBuffer {
    messages: Vec<Vec<u8>>,
}

/// WebSocket client (non-Send resource for Bevy).
/// Uses Rc<RefCell> because WASM is single-threaded.
pub struct WsClient {
    #[cfg(target_family = "wasm")]
    ws: Option<web_sys::WebSocket>,
    buffer: Rc<RefCell<MessageBuffer>>,
    connected: Rc<RefCell<bool>>,
    /// Messages queued while WebSocket is still connecting (WASM only).
    #[cfg(target_family = "wasm")]
    outbound_queue: Rc<RefCell<Vec<Vec<u8>>>>,
}

impl Default for WsClient {
    fn default() -> Self {
        Self::new()
    }
}

impl WsClient {
    pub fn new() -> Self {
        Self {
            #[cfg(target_family = "wasm")]
            ws: None,
            buffer: Rc::new(RefCell::new(MessageBuffer::default())),
            connected: Rc::new(RefCell::new(false)),
            #[cfg(target_family = "wasm")]
            outbound_queue: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Connect to the server WebSocket.
    #[cfg(target_family = "wasm")]
    pub fn connect(&mut self, url: &str) -> Result<(), String> {
        let ws = web_sys::WebSocket::new(url).map_err(|e| format!("WebSocket error: {e:?}"))?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let buffer = Rc::clone(&self.buffer);
        let onmessage =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |evt: web_sys::MessageEvent| {
                if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buf);
                    let data = array.to_vec();
                    buffer.borrow_mut().messages.push(data);
                }
            });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        onmessage.forget();

        let connected = Rc::clone(&self.connected);
        let queue = Rc::clone(&self.outbound_queue);
        let ws_clone = ws.clone();
        let onopen = Closure::<dyn FnMut()>::new(move || {
            *connected.borrow_mut() = true;
            web_sys::console::log_1(&"WebSocket connected".into());
            // Flush any messages queued while connecting
            let queued: Vec<Vec<u8>> = queue.borrow_mut().drain(..).collect();
            for data in queued {
                let _ = ws_clone.send_with_u8_array(&data);
            }
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let connected_err = Rc::clone(&self.connected);
        let onerror =
            Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |_: web_sys::ErrorEvent| {
                *connected_err.borrow_mut() = false;
                web_sys::console::log_1(&"WebSocket error".into());
            });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        let connected_close = Rc::clone(&self.connected);
        let onclose =
            Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |_: web_sys::CloseEvent| {
                *connected_close.borrow_mut() = false;
                web_sys::console::log_1(&"WebSocket closed".into());
            });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        self.ws = Some(ws);
        Ok(())
    }

    /// Stub for non-WASM targets (native check only).
    #[cfg(not(target_family = "wasm"))]
    pub fn connect(&mut self, _url: &str) -> Result<(), String> {
        *self.connected.borrow_mut() = true;
        Ok(())
    }

    /// Send raw binary data over the WebSocket.
    /// If the WebSocket exists but is still connecting, the message is queued
    /// and will be sent automatically when the connection opens.
    #[cfg(target_family = "wasm")]
    pub fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Some(ws) = &self.ws {
            if *self.connected.borrow() {
                ws.send_with_u8_array(data)
                    .map_err(|e| format!("Send error: {e:?}"))
            } else {
                // WebSocket exists but is still connecting — queue the message
                self.outbound_queue.borrow_mut().push(data.to_vec());
                Ok(())
            }
        } else {
            Err("Not connected".to_string())
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn send(&self, _data: &[u8]) -> Result<(), String> {
        Ok(())
    }

    /// Drain all buffered messages.
    pub fn drain_messages(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.buffer.borrow_mut().messages)
    }

    /// Check if the WebSocket is connected.
    pub fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

    /// Check if the WebSocket exists (connecting or connected).
    pub fn has_connection(&self) -> bool {
        #[cfg(target_family = "wasm")]
        {
            self.ws.is_some()
        }
        #[cfg(not(target_family = "wasm"))]
        {
            *self.connected.borrow()
        }
    }
}

// ── Connection Status Indicator ──────────────────────────────────

pub struct ConnectionStatusPlugin;

impl Plugin for ConnectionStatusPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ConnectionStatus::default())
            .add_systems(Update, connection_monitor_system);
    }
}

/// Tracks whether the WebSocket was previously connected (to detect disconnects).
#[derive(Resource, Default)]
pub struct ConnectionStatus {
    was_connected: bool,
    banner_entity: Option<Entity>,
}

/// Marker for the disconnect banner UI.
#[derive(Component)]
struct DisconnectBanner;

fn connection_monitor_system(
    ws_client: NonSend<WsClient>,
    mut status: ResMut<ConnectionStatus>,
    mut commands: Commands,
    banner_query: Query<Entity, With<DisconnectBanner>>,
) {
    let connected = ws_client.is_connected();

    // Detect disconnect: was connected, now isn't, and connection exists (not initial state)
    if status.was_connected && !connected && ws_client.has_connection() {
        // Show disconnect banner if not already visible
        if status.banner_entity.is_none() {
            let entity = commands
                .spawn((
                    DisconnectBanner,
                    Node {
                        position_type: PositionType::Absolute,
                        top: Val::Px(0.0),
                        left: Val::Px(0.0),
                        width: Val::Percent(100.0),
                        padding: UiRect::axes(Val::Px(16.0), Val::Px(8.0)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.8, 0.1, 0.1, 0.9)),
                    GlobalZIndex(100),
                ))
                .with_child((
                    Text::new("Disconnected — refresh page to reconnect"),
                    TextFont {
                        font_size: 16.0,
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ))
                .id();
            status.banner_entity = Some(entity);
        }
    }

    // If reconnected, remove banner
    if connected && status.banner_entity.is_some() {
        for entity in &banner_query {
            commands.entity(entity).despawn();
        }
        status.banner_entity = None;
    }

    status.was_connected = connected;
}
