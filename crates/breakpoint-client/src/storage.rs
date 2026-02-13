/// Calls the closure with browser localStorage, if available. No-op on non-WASM.
#[allow(unused_variables)]
pub fn with_local_storage(f: impl FnOnce(&web_sys::Storage)) {
    #[cfg(target_family = "wasm")]
    {
        if let Some(window) = web_sys::window()
            && let Ok(Some(storage)) = window.local_storage()
        {
            f(&storage);
        }
    }
}
