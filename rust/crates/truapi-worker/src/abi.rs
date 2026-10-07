//! The sandbox boundary, called only by the exports [`export_worker!`]
//! generates. Bytes cross through guest linear memory; the host allocates
//! with [`alloc`] before an [`on_frame`] and frees with [`free`] after it.
//!
//! [`export_worker!`]: crate::export_worker

use core::cell::RefCell;

use crate::{Frame, Instance, ProductWorker};

/// One instance behind the exports, whatever the product's type.
pub trait Turns {
    /// See [`Instance::start`].
    fn start(&mut self) -> Vec<Frame>;
    /// See [`Instance::on_frame`].
    fn frame(&mut self, bytes: &[u8]) -> Vec<Frame>;
    /// See [`Instance::on_suspend`].
    fn suspend(&mut self) -> Vec<Frame>;
    /// See [`Instance::on_resume`].
    fn resume(&mut self) -> Vec<Frame>;
    /// See [`Instance::take_logs`].
    fn take_logs(&mut self) -> Vec<String>;
}

impl<W: ProductWorker> Turns for Instance<W> {
    fn start(&mut self) -> Vec<Frame> {
        Instance::start(self)
    }

    fn frame(&mut self, bytes: &[u8]) -> Vec<Frame> {
        self.on_frame(bytes)
    }

    fn suspend(&mut self) -> Vec<Frame> {
        self.on_suspend()
    }

    fn resume(&mut self) -> Vec<Frame> {
        self.on_resume()
    }

    fn take_logs(&mut self) -> Vec<String> {
        Instance::take_logs(self)
    }
}

thread_local! {
    static INSTANCE: RefCell<Option<Box<dyn Turns>>> = const { RefCell::new(None) };
}

#[link(wasm_import_module = "host")]
unsafe extern "C" {
    /// Hand one encoded frame to the host; it copies the bytes before returning.
    fn frame_send(ptr: *const u8, len: u32);
    /// Record one line of guest evidence; the host copies the bytes before returning.
    fn log(ptr: *const u8, len: u32);
}

/// Reserve `len` bytes the host may write a frame into.
pub fn alloc(len: u32) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len as usize);
    let ptr = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    ptr
}

/// Release a buffer that [`alloc`] reserved, once the host is done with it.
///
/// # Safety
/// `ptr` and `len` must come from one [`alloc`] call and not be freed twice.
pub unsafe fn free(ptr: *mut u8, len: u32) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len as usize) });
}

/// The connection is open.
pub fn on_start(instance: fn() -> Box<dyn Turns>) {
    drive(instance, |turns| turns.start());
}

/// One frame from the host, `len` bytes at `ptr`.
///
/// # Safety
/// `ptr` must point at `len` readable bytes in guest memory.
pub unsafe fn on_frame(instance: fn() -> Box<dyn Turns>, ptr: *const u8, len: u32) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len as usize) };
    drive(instance, |turns| turns.frame(bytes));
}

/// The host paused the worker.
pub fn on_suspend(instance: fn() -> Box<dyn Turns>) {
    drive(instance, |turns| turns.suspend());
}

/// The host resumed the worker.
pub fn on_resume(instance: fn() -> Box<dyn Turns>) {
    drive(instance, |turns| turns.resume());
}

/// Run one turn on the instance, building it on the first, then hand every
/// frame and then every log line to the host.
fn drive(instance: fn() -> Box<dyn Turns>, step: impl FnOnce(&mut dyn Turns) -> Vec<Frame>) {
    INSTANCE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let turns = slot.get_or_insert_with(instance).as_mut();
        for frame in step(turns) {
            let bytes = frame.encode();
            unsafe { frame_send(bytes.as_ptr(), bytes.len() as u32) };
        }
        for line in turns.take_logs() {
            unsafe { log(line.as_ptr(), line.len() as u32) };
        }
    });
}
