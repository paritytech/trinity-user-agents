//! The QuickJS guest translated to C by wasm2c and compiled into this crate:
//! the wasm sandbox with no interpreter and no JIT, which is the form iOS
//! allows. Same guest, imports and RESULT JSON as `qjs-runner`; shared by
//! the `qjs-runner-wasm2c` binary and the iOS bench.
//!
//! There is no fuel: wasm2c has no instruction metering, so a runaway script
//! is not stopped here. The memory cap lowers the guest memory's maximum.
//! Every call runs on the calling thread, which wasm2c prepares on first use
//! (signal handler and alternate signal stack); call [`release_thread`]
//! before that thread exits.

use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr},
    time::{Duration, Instant},
};

/// Which wasm2c build this is, such as `wasm2c-bounds-O2`.
pub const VARIANT: &str = env!("QJS_W2C_VARIANT");
const PAGE_BYTES: usize = 64 << 10;
/// What a guard-page instance reserves: 4 GiB of memory and 4 GiB of guard,
/// as `get_alloc_size_for_mmap_default32` in the wasm2c runtime.
const GUARD_RESERVATION_BYTES: usize = 8 << 30;

#[repr(C)]
struct RawInstance {
    _private: [u8; 0],
}

extern "C" {
    fn qjs_runtime_init();
    fn qjs_runtime_free_thread();
    fn qjs_new(host: *mut c_void, max_pages: u64, out: *mut *mut RawInstance) -> i32;
    fn qjs_free(instance: *mut RawInstance);
    fn qjs_memory_bytes(instance: *mut RawInstance) -> u64;
    fn qjs_write(instance: *mut RawInstance, pointer: u32, bytes: *const u8, len: u32) -> i32;
    fn qjs_alloc(instance: *mut RawInstance, len: u32, pointer: *mut u32) -> i32;
    fn qjs_dealloc(instance: *mut RawInstance, pointer: u32, len: u32) -> i32;
    fn qjs_init(instance: *mut RawInstance, pointer: u32, len: u32, status: *mut u32) -> i32;
    fn qjs_on_frame(instance: *mut RawInstance, pointer: u32, len: u32) -> i32;
    fn qjs_on_timer(instance: *mut RawInstance, id: u32) -> i32;
    fn qjs_run_jobs(instance: *mut RawInstance, ran: *mut u32) -> i32;
    fn qjs_trap_name(trap: i32) -> *const c_char;
}

/// What every guest a run boots is allowed.
pub struct Limits {
    /// The guest's linear memory cap.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { memory_bytes: 64 << 20 }
    }
}

thread_local! {
    static READY: Cell<bool> = const { Cell::new(false) };
}

fn prepare_thread() {
    if READY.get() {
        return;
    }
    // SAFETY: installs the signal handler once and this thread's alt stack.
    unsafe { qjs_runtime_init() };
    READY.set(true);
}

/// Free this thread's wasm2c state. Call it before the thread exits: Rust
/// replaces the thread's alternate signal stack during its own teardown, so a
/// thread-local destructor would run too late.
pub fn release_thread() {
    if READY.replace(false) {
        // SAFETY: paired with the `qjs_runtime_init` on this same thread.
        unsafe { qjs_runtime_free_thread() };
    }
}

struct HostState {
    started: Instant,
    timers: BTreeMap<u32, Instant>,
    outbound: Vec<Vec<u8>>,
    echo: bool,
}

fn state<'a>(host: *mut c_void) -> &'a mut HostState {
    // SAFETY: `host` is the `Box<HostState>` a `Guest` owns, and the guest
    // calls back only while the host is inside one of its exports.
    unsafe { &mut *host.cast::<HostState>() }
}

fn bytes<'a>(pointer: *const u8, len: usize) -> &'a [u8] {
    // SAFETY: the glue checked `pointer..pointer+len` against guest memory.
    unsafe { std::slice::from_raw_parts(pointer, len) }
}

#[no_mangle]
extern "C" fn qjs_host_frame_out(host: *mut c_void, pointer: *const u8, len: usize) {
    state(host).outbound.push(bytes(pointer, len).to_vec());
}

#[no_mangle]
extern "C" fn qjs_host_log(_host: *mut c_void, level: u32, pointer: *const u8, len: usize) {
    bench_common::log_line(level, &String::from_utf8_lossy(bytes(pointer, len)));
}

#[no_mangle]
extern "C" fn qjs_host_now_ms(host: *mut c_void) -> f64 {
    state(host).started.elapsed().as_secs_f64() * 1000.0
}

#[no_mangle]
extern "C" fn qjs_host_random_fill(pointer: *mut u8, len: usize) {
    // SAFETY: the glue checked the range against guest memory.
    let target = unsafe { std::slice::from_raw_parts_mut(pointer, len) };
    getrandom::fill(target).expect("random bytes");
}

#[no_mangle]
extern "C" fn qjs_host_timer_set(host: *mut c_void, id: u32, delay_ms: f64) {
    let due = Instant::now() + Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
    state(host).timers.insert(id, due);
}

#[no_mangle]
extern "C" fn qjs_host_timer_clear(host: *mut c_void, id: u32) {
    state(host).timers.remove(&id);
}

fn check(trap: i32) -> Result<(), String> {
    if trap == 0 {
        return Ok(());
    }
    // SAFETY: wasm_rt_strerror returns a static C string.
    let name = unsafe { CStr::from_ptr(qjs_trap_name(trap)) };
    Err(format!("guest trapped: {}", name.to_string_lossy()))
}

struct Guest {
    instance: *mut RawInstance,
    host: *mut HostState,
}

impl Drop for Guest {
    fn drop(&mut self) {
        // SAFETY: both were created in `Guest::boot` and are freed once here.
        unsafe {
            qjs_free(self.instance);
            drop(Box::from_raw(self.host));
        }
    }
}

impl Guest {
    fn boot(source: &[u8], echo: bool, limits: &Limits) -> Result<(Self, Duration), String> {
        prepare_thread();
        let started = Instant::now();
        let host = Box::into_raw(Box::new(HostState {
            started: Instant::now(),
            timers: BTreeMap::new(),
            outbound: Vec::new(),
            echo,
        }));
        let mut instance = std::ptr::null_mut();
        let max_pages = (limits.memory_bytes / PAGE_BYTES) as u64;
        // SAFETY: `host` stays alive for the guest's lifetime (see `Drop`).
        let created = unsafe { qjs_new(host.cast(), max_pages, &mut instance) };
        if let Err(error) = check(created) {
            // SAFETY: the instance was never created, so nothing else holds `host`.
            drop(unsafe { Box::from_raw(host) });
            return Err(error);
        }
        let mut guest = Guest { instance, host };
        let pointer = guest.write_bytes(source)?;
        let mut status = 0;
        // SAFETY: `instance` is live; the out-pointer is a local.
        check(unsafe { qjs_init(guest.instance, pointer, source.len() as u32, &mut status) })?;
        guest.free_bytes(pointer, source.len())?;
        guest.settle()?;
        if status != 0 {
            return Err(format!("init returned {status}"));
        }
        Ok((guest, started.elapsed()))
    }

    fn host(&mut self) -> &mut HostState {
        state(self.host.cast())
    }

    fn write_bytes(&mut self, source: &[u8]) -> Result<u32, String> {
        let mut pointer = 0;
        // SAFETY: `instance` is live; the glue bounds-checks the write.
        unsafe {
            check(qjs_alloc(self.instance, source.len() as u32, &mut pointer))?;
            check(qjs_write(self.instance, pointer, source.as_ptr(), source.len() as u32))?;
        }
        Ok(pointer)
    }

    fn free_bytes(&mut self, pointer: u32, len: usize) -> Result<(), String> {
        // SAFETY: `instance` is live.
        check(unsafe { qjs_dealloc(self.instance, pointer, len as u32) })
    }

    fn deliver(&mut self, frame: &[u8]) -> Result<(), String> {
        let pointer = self.write_bytes(frame)?;
        // SAFETY: `instance` is live.
        check(unsafe { qjs_on_frame(self.instance, pointer, frame.len() as u32) })?;
        self.free_bytes(pointer, frame.len())
    }

    fn fire_timer(&mut self, id: u32) -> Result<(), String> {
        // SAFETY: `instance` is live.
        check(unsafe { qjs_on_timer(self.instance, id) })
    }

    fn run_jobs(&mut self) -> Result<(), String> {
        let mut ran = 0;
        // SAFETY: `instance` is live; the out-pointer is a local.
        check(unsafe { qjs_run_jobs(self.instance, &mut ran) })
    }

    fn settle(&mut self) -> Result<(), String> {
        loop {
            self.run_jobs()?;
            if !self.host().echo {
                return Ok(());
            }
            let outbound = std::mem::take(&mut self.host().outbound);
            if outbound.is_empty() {
                return Ok(());
            }
            for frame in outbound {
                self.deliver(&frame)?;
            }
        }
    }

    fn memory_kib(&self) -> u64 {
        // SAFETY: `instance` is live.
        unsafe { qjs_memory_bytes(self.instance) / 1024 }
    }

    fn next_timer(&mut self) -> Option<(u32, Instant)> {
        self.host().timers.iter().min_by_key(|(_, due)| **due).map(|(id, due)| (*id, *due))
    }
}

/// Run `source` to completion with frames echoed back: evaluate, drain
/// jobs, fire timers on wall-clock time. Returns the time from instantiation
/// to idle.
pub fn run_to_completion(source: &[u8], limits: &Limits) -> Result<Duration, String> {
    let (mut guest, init_in) = Guest::boot(source, true, limits)?;
    eprintln!("[runner] {VARIANT}: instantiate and init to idle {init_in:?}");
    while let Some((id, due)) = guest.next_timer() {
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        guest.host().timers.remove(&id);
        guest.fire_timer(id)?;
        guest.settle()?;
    }
    eprintln!("[runner] idle, no timers left; linear memory {} KiB", guest.memory_kib());
    Ok(init_in)
}

/// Boot `count` guests on the worker bundle, keeping each alive, and report
/// cold start and the memory each one adds to the process.
pub fn instances(source: &[u8], count: usize, limits: &Limits) -> Result<String, String> {
    prepare_thread();
    let footprint_before = bench_common::footprint_bytes();
    let mut guests = Vec::with_capacity(count);
    let mut boot_ms = Vec::with_capacity(count);
    let mut footprints = Vec::with_capacity(count);
    for _ in 0..count {
        let (guest, booted_in) = Guest::boot(source, false, limits)?;
        boot_ms.push(booted_in.as_secs_f64() * 1000.0);
        footprints.push(bench_common::footprint_bytes());
        guests.push(guest);
    }
    let first_mib = bench_common::mib(footprints[0] - footprint_before);
    let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
    let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
    Ok(format!(
        "{{\"engine\":\"{VARIANT}\",\"firstBootMs\":{:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"linearMemoryKiB\":{},\"instances\":{count}}}",
        boot_ms[0],
        guests[0].memory_kib(),
    ))
}

/// Replay the corpus against a fresh guest, checking every outbound frame
/// byte for byte and timing each render request until its tree comes back.
pub fn replay(source: &[u8], corpus: &str, limits: &Limits) -> Result<String, String> {
    let (mut guest, booted_in) = Guest::boot(source, false, limits)?;
    let steps = bench_common::load_corpus(corpus);
    let mut render_ms = Vec::new();
    let mut mismatches = 0;
    for step in &steps {
        let started = Instant::now();
        guest.deliver(&step.inbound)?;
        guest.settle()?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let sent = std::mem::take(&mut guest.host().outbound);
        if sent != step.outbound {
            mismatches += 1;
        }
        if bench_common::is_render_start(&step.inbound) {
            render_ms.push(elapsed);
        }
    }
    let (p50, p95) = bench_common::percentiles(&mut render_ms);
    Ok(format!(
        "{{\"engine\":\"{VARIANT}\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3},\"linearMemoryKiB\":{}}}",
        booted_in.as_secs_f64() * 1000.0,
        steps.len(),
        render_ms.len(),
        guest.memory_kib(),
    ))
}

/// Make the guest read past its linear memory and check the sandbox catches
/// it: the guest copies a frame from an address near 4 GiB, which traps as
/// out of bounds (through the signal handler with guard pages, through the
/// explicit check otherwise). The trapped instance is discarded, as a host
/// must, and a fresh one boots to show the process is unharmed.
pub fn oob_trap(source: &[u8], limits: &Limits) -> Result<String, String> {
    let (guest, _) = Guest::boot(source, false, limits)?;
    // SAFETY: `instance` is live; the address is outside guest memory on purpose.
    let trap = unsafe { qjs_on_frame(guest.instance, 0xFFFF_0000, 0x1000) };
    drop(guest);
    let trapped = match check(trap) {
        Ok(()) => return Err("the out-of-bounds read did not trap".into()),
        Err(error) => error,
    };
    let (_fresh, booted_in) = Guest::boot(source, false, limits)?;
    Ok(format!(
        "{{\"engine\":\"{VARIANT}\",\"probe\":\"oobTrap\",\"trapped\":\"{trapped}\",\"recovered\":true,\"freshBootMs\":{:.1}}}",
        booted_in.as_secs_f64() * 1000.0
    ))
}

/// How many guard-page instances this process could hold: reserve the
/// address space each one takes, `PROT_NONE` as the wasm2c runtime does,
/// until a reservation fails or `limit` is reached, then release them all.
/// Probed directly because the runtime aborts the process when one fails.
pub fn guard_capacity(limit: usize) -> String {
    let mut reservations = Vec::new();
    while reservations.len() < limit {
        // SAFETY: an anonymous PROT_NONE mapping touches no existing memory.
        let address = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                GUARD_RESERVATION_BYTES,
                libc::PROT_NONE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        if address == libc::MAP_FAILED {
            break;
        }
        reservations.push(address);
    }
    let count = reservations.len();
    for address in reservations {
        // SAFETY: each address is a mapping of this size made above.
        unsafe { libc::munmap(address, GUARD_RESERVATION_BYTES) };
    }
    format!(
        "{{\"engine\":\"{VARIANT}\",\"probe\":\"guardCapacity\",\"reservationGiB\":8,\"instances\":{count},\"limit\":{limit}}}"
    )
}
