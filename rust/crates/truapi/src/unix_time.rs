//! Unix-time source shared by the statement builders and readers.

/// Current unix time in seconds, used to stamp outgoing statement expiries
/// and to gate inbound statement freshness. Trusts the local clock on both
/// native and wasm targets.
#[cfg(not(target_arch = "wasm32"))]
pub fn current_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Current unix time in seconds on wasm32, sourced from the JS clock.
#[cfg(target_arch = "wasm32")]
pub fn current_unix_secs() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Current unix time in milliseconds.
#[cfg(not(target_arch = "wasm32"))]
pub fn current_unix_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Current unix time in milliseconds on wasm32, sourced from the JS clock.
#[cfg(target_arch = "wasm32")]
pub fn current_unix_millis() -> u64 {
    js_sys::Date::now() as u64
}
