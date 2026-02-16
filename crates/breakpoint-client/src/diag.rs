/// Logging macros that route to the browser console in WASM and are no-ops in
/// native/test builds.  Called as `diag::console_warn!("msg: {e}")` from
/// sibling modules.

#[cfg(target_family = "wasm")]
macro_rules! console_warn {
    ($($arg:tt)*) => {
        web_sys::console::warn_1(&format!($($arg)*).into())
    };
}

#[cfg(not(target_family = "wasm"))]
macro_rules! console_warn {
    ($($arg:tt)*) => {
        if false { let _ = format_args!($($arg)*); }
    };
}

pub(crate) use console_warn;
