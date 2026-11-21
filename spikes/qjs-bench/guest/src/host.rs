//! Safe wrappers over the `truapi` host imports.

pub const LEVEL_ERROR: u32 = 3;

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

pub fn frame_out(frame: &[u8]) {
    unsafe { imports::frame_out(frame.as_ptr(), frame.len()) }
}

pub fn now_ms() -> f64 {
    unsafe { imports::now_ms() }
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    unsafe { imports::random_fill(bytes.as_mut_ptr(), bytes.len()) };
    bytes
}

pub fn timer_set(id: u32, delay_ms: f64) {
    unsafe { imports::timer_set(id, delay_ms) }
}

pub fn timer_clear(id: u32) {
    unsafe { imports::timer_clear(id) }
}

pub fn log(level: u32, message: &str) {
    unsafe { imports::log(level, message.as_ptr(), message.len()) }
}
