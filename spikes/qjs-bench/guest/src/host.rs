//! The `truapi` host imports. In wasm they are real imports; natively (E2)
//! they are implemented in-process, so the same guest code runs both ways.

pub const LEVEL_ERROR: u32 = 3;

#[cfg(target_arch = "wasm32")]
mod imports {
    #[link(wasm_import_module = "truapi")]
    extern "C" {
        pub fn frame_out(pointer: *const u8, len: usize);
        pub fn now_ms() -> f64;
        pub fn random_fill(pointer: *mut u8, len: usize);
        pub fn timer_set(id: u32, delay_ms: f64);
        pub fn timer_clear(id: u32);
        pub fn log(level: u32, pointer: *const u8, len: usize);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn frame_out(frame: &[u8]) {
    unsafe { imports::frame_out(frame.as_ptr(), frame.len()) }
}

#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    unsafe { imports::now_ms() }
}

#[cfg(target_arch = "wasm32")]
pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    unsafe { imports::random_fill(bytes.as_mut_ptr(), bytes.len()) };
    bytes
}

#[cfg(target_arch = "wasm32")]
pub fn timer_set(id: u32, delay_ms: f64) {
    unsafe { imports::timer_set(id, delay_ms) }
}

#[cfg(target_arch = "wasm32")]
pub fn timer_clear(id: u32) {
    unsafe { imports::timer_clear(id) }
}

#[cfg(target_arch = "wasm32")]
pub fn log(level: u32, message: &str) {
    unsafe { imports::log(level, message.as_ptr(), message.len()) }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

/// In-process host for native runs: frames and timers are queued for the
/// driver to take with [`native::take_outbound`] and [`native::take_timers`].
#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    use std::{cell::RefCell, collections::BTreeMap, time::Instant};

    thread_local! {
        static STARTED: Instant = Instant::now();
        static OUTBOUND: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
        static TIMERS: RefCell<BTreeMap<u32, f64>> = const { RefCell::new(BTreeMap::new()) };
    }

    pub fn frame_out(frame: &[u8]) {
        OUTBOUND.with_borrow_mut(|outbound| outbound.push(frame.to_vec()));
    }

    pub fn now_ms() -> f64 {
        STARTED.with(|started| started.elapsed().as_secs_f64() * 1000.0)
    }

    pub fn random_bytes(len: usize) -> Vec<u8> {
        let mut bytes = vec![0; len];
        getrandom::fill(&mut bytes).expect("random bytes");
        bytes
    }

    pub fn timer_set(id: u32, delay_ms: f64) {
        let due = now_ms() + delay_ms.max(0.0);
        TIMERS.with_borrow_mut(|timers| timers.insert(id, due));
    }

    pub fn timer_clear(id: u32) {
        TIMERS.with_borrow_mut(|timers| timers.remove(&id));
    }

    pub fn log(level: u32, message: &str) {
        let label = ["debug", "info", "warn", "error"].get(level as usize).copied().unwrap_or("log");
        println!("[{label}] {message}");
    }

    pub fn take_outbound() -> Vec<Vec<u8>> {
        OUTBOUND.with_borrow_mut(std::mem::take)
    }

    /// Pending timers as `(id, due in ms since start)`.
    pub fn take_timers() -> BTreeMap<u32, f64> {
        TIMERS.with_borrow_mut(std::mem::take)
    }
}
