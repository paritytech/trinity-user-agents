//! The sandbox boundary: C-ABI exports the embedder calls and the two imports
//! the guest calls back. Bytes cross through guest linear memory; the
//! embedder allocates with [`alloc`] before an [`on_frame`] and frees what the
//! guest handed out after copying it. The guest never blocks on the host.

use std::cell::RefCell;

use crate::Worker;

thread_local! {
    static WORKER: RefCell<Worker> = RefCell::new(fresh_worker());
}

#[cfg(all(feature = "pocket", feature = "unified"))]
compile_error!("`pocket` and `unified` select different modes; enable one");

#[cfg(feature = "unified")]
fn fresh_worker() -> Worker {
    Worker::unified()
}

#[cfg(all(feature = "pocket", not(feature = "unified")))]
fn fresh_worker() -> Worker {
    Worker::pocket()
}

#[cfg(not(any(feature = "pocket", feature = "unified")))]
fn fresh_worker() -> Worker {
    Worker::new()
}

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    /// Hand one encoded frame to the host; it copies the bytes before returning.
    fn frame_send(ptr: *const u8, len: u32);
    /// Record one line of guest evidence; the host copies the bytes before returning.
    fn log(ptr: *const u8, len: u32);
}

/// Reserve `len` bytes the embedder may write a frame into.
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: u32) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len as usize);
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    ptr
}

/// Release a buffer that [`alloc`] reserved, once the embedder is done with it.
///
/// # Safety
/// `ptr` and `len` must come from one [`alloc`] call and not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut u8, len: u32) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len as usize) });
}

/// The connection is open: send the opening frames.
#[unsafe(no_mangle)]
pub extern "C" fn on_start() {
    drive(|worker| worker.start());
}

/// One frame from the host, `len` bytes at `ptr`.
///
/// # Safety
/// `ptr` must point at `len` readable bytes in guest memory.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn on_frame(ptr: *const u8, len: u32) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    drive(|worker| worker.on_frame(bytes));
}

/// The host paused the worker.
#[unsafe(no_mangle)]
pub extern "C" fn on_suspend() {
    drive(Worker::on_suspend);
}

/// The host resumed the worker.
#[unsafe(no_mangle)]
pub extern "C" fn on_resume() {
    drive(Worker::on_resume);
}

fn drive(step: impl FnOnce(&mut Worker) -> Vec<crate::Frame>) {
    WORKER.with(|worker| {
        let mut worker = worker.borrow_mut();
        for frame in step(&mut worker) {
            let bytes = frame.encode();
            unsafe { frame_send(bytes.as_ptr(), bytes.len() as u32) };
        }
        for event in worker.take_events() {
            let line = format!("{event:?}");
            unsafe { log(line.as_ptr(), line.len() as u32) };
        }
    });
}
