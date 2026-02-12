use std::cell::RefCell;
use std::rc::Rc;

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
        let onopen = Closure::<dyn FnMut()>::new(move || {
            *connected.borrow_mut() = true;
            web_sys::console::log_1(&"WebSocket connected".into());
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
    #[cfg(target_family = "wasm")]
    pub fn send(&self, data: &[u8]) -> Result<(), String> {
        if let Some(ws) = &self.ws {
            ws.send_with_u8_array(data)
                .map_err(|e| format!("Send error: {e:?}"))
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
}
