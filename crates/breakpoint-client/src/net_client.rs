use std::cell::RefCell;
use std::rc::Rc;

#[cfg(target_family = "wasm")]
use wasm_bindgen::JsCast;
#[cfg(target_family = "wasm")]
use wasm_bindgen::closure::Closure;

/// Buffer for messages received from the WebSocket.
#[derive(Default)]
struct MessageBuffer {
    messages: Vec<Vec<u8>>,
}

/// Stored WebSocket event handler closures.
/// Prevents the `.forget()` memory leak — closures are dropped when the
/// connection is closed via [`WsClient::disconnect`].
#[cfg(target_family = "wasm")]
struct WsClosures {
    _onmessage: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _onopen: Closure<dyn FnMut()>,
    _onerror: Closure<dyn FnMut(web_sys::ErrorEvent)>,
    _onclose: Closure<dyn FnMut(web_sys::CloseEvent)>,
}

/// WebSocket client.
/// Uses Rc<RefCell> because WASM is single-threaded.
pub struct WsClient {
    #[cfg(target_family = "wasm")]
    ws: Option<web_sys::WebSocket>,
    #[cfg(target_family = "wasm")]
    closures: Option<WsClosures>,
    buffer: Rc<RefCell<MessageBuffer>>,
    connected: Rc<RefCell<bool>>,
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
            #[cfg(target_family = "wasm")]
            closures: None,
            buffer: Rc::new(RefCell::new(MessageBuffer::default())),
            connected: Rc::new(RefCell::new(false)),
            #[cfg(target_family = "wasm")]
            outbound_queue: Rc::new(RefCell::new(Vec::new())),
        }
    }

    #[cfg(target_family = "wasm")]
    pub fn connect(&mut self, url: &str) -> Result<(), String> {
        // Clean up any existing connection first
        self.disconnect();

        let ws = web_sys::WebSocket::new(url).map_err(|e| format!("WebSocket error: {e:?}"))?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

        // onmessage: push binary data into the shared buffer
        let buffer = Rc::clone(&self.buffer);
        let onmessage =
            Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |evt: web_sys::MessageEvent| {
                if let Ok(buf) = evt.data().dyn_into::<js_sys::ArrayBuffer>() {
                    let array = js_sys::Uint8Array::new(&buf);
                    let data = array.to_vec();
                    buffer.borrow_mut().messages.push(data);
                } else {
                    web_sys::console::warn_1(
                        &"WebSocket received non-binary message, ignoring".into(),
                    );
                }
            });
        ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));

        // onopen: mark connected, cancel timeout, and flush queued messages
        let connected = Rc::clone(&self.connected);
        let queue = Rc::clone(&self.outbound_queue);
        let ws_clone = ws.clone();
        let timeout_id = Rc::new(RefCell::new(0i32));
        let timeout_id_open = Rc::clone(&timeout_id);
        let onopen = Closure::<dyn FnMut()>::new(move || {
            *connected.borrow_mut() = true;
            // Cancel the connection timeout
            if let Some(window) = web_sys::window() {
                window.clear_timeout_with_handle(*timeout_id_open.borrow());
            }
            web_sys::console::log_1(&"WebSocket connected".into());
            let queued: Vec<Vec<u8>> = queue.borrow_mut().drain(..).collect();
            if !queued.is_empty() {
                web_sys::console::log_1(
                    &format!("Flushing {} queued messages", queued.len()).into(),
                );
            }
            for data in queued {
                if let Err(e) = ws_clone.send_with_u8_array(&data) {
                    web_sys::console::warn_1(
                        &format!(
                            "Failed to flush queued message ({} bytes): {e:?}",
                            data.len()
                        )
                        .into(),
                    );
                }
            }
        });
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));

        // onerror
        let connected_err = Rc::clone(&self.connected);
        let onerror =
            Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |evt: web_sys::ErrorEvent| {
                *connected_err.borrow_mut() = false;
                web_sys::console::error_1(&format!("WebSocket error: {}", evt.message()).into());
            });
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));

        // onclose
        let connected_close = Rc::clone(&self.connected);
        let onclose =
            Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |evt: web_sys::CloseEvent| {
                *connected_close.borrow_mut() = false;
                web_sys::console::warn_1(
                    &format!(
                        "WebSocket closed: code={}, reason='{}'",
                        evt.code(),
                        evt.reason()
                    )
                    .into(),
                );
            });
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        // Store closures instead of .forget() — they are dropped in disconnect()
        self.closures = Some(WsClosures {
            _onmessage: onmessage,
            _onopen: onopen,
            _onerror: onerror,
            _onclose: onclose,
        });

        // Set a 10-second connection timeout — if onopen doesn't fire, close the socket
        let ws_timeout = ws.clone();
        let connected_timeout = Rc::clone(&self.connected);
        if let Some(window) = web_sys::window() {
            let timeout_closure = Closure::<dyn FnMut()>::once(move || {
                if !*connected_timeout.borrow() {
                    web_sys::console::warn_1(&"WebSocket connection timed out after 10s".into());
                    let _ = ws_timeout.close();
                }
            });
            if let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                timeout_closure.as_ref().unchecked_ref(),
                10_000,
            ) {
                *timeout_id.borrow_mut() = id;
            }
            timeout_closure.forget();
        }

        self.ws = Some(ws);
        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn connect(&mut self, _url: &str) -> Result<(), String> {
        *self.connected.borrow_mut() = true;
        Ok(())
    }

    /// Cleanly close the WebSocket and free event handler closures.
    #[cfg(target_family = "wasm")]
    pub fn disconnect(&mut self) {
        *self.connected.borrow_mut() = false;
        if let Some(ws) = self.ws.take() {
            // Clear handlers before closing to prevent callbacks during teardown
            ws.set_onmessage(None);
            ws.set_onopen(None);
            ws.set_onerror(None);
            ws.set_onclose(None);
            let _ = ws.close();
        }
        // Drop closures (frees WASM-JS trampolines)
        self.closures = None;
        // Preserve outbound queue for reconnection — onopen will flush it.
        // Only discard messages older than a reasonable window (keep last 32).
        let mut queue = self.outbound_queue.borrow_mut();
        if queue.len() > 32 {
            queue.drain(..queue.len() - 32);
        }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn disconnect(&mut self) {
        *self.connected.borrow_mut() = false;
    }

    #[cfg(target_family = "wasm")]
    pub fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Some(ws) = &self.ws {
            if *self.connected.borrow() {
                ws.send_with_u8_array(data)
                    .map_err(|e| format!("Send error: {e:?}"))
            } else {
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

    pub fn drain_messages(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.buffer.borrow_mut().messages)
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.borrow()
    }

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
