//! The QuickJS guest translated to C by wasm2c and compiled into this
//! binary: the wasm sandbox with no interpreter and no JIT, which is the form
//! iOS allows. Same guest, imports, modes and RESULT lines as `qjs-runner`.
//!
//! Usage: qjs-runner-wasm2c <script.js> [--memory-mib N] [--native-stack-mib N]
//!        [--replay corpus.bin] [--instances N]
//!
//! There is no fuel: wasm2c has no instruction metering, so a runaway script
//! is not stopped here. The memory cap lowers the guest memory's maximum.

use std::{
    collections::BTreeMap,
    ffi::{CStr, c_char, c_void},
    process::ExitCode,
    time::{Duration, Instant},
};

const VARIANT: &str = env!("QJS_W2C_VARIANT");
const PAGE_BYTES: usize = 64 << 10;

#[repr(C)]
struct RawInstance {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn qjs_runtime_init();
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

#[unsafe(no_mangle)]
extern "C" fn qjs_host_frame_out(host: *mut c_void, pointer: *const u8, len: usize) {
    state(host).outbound.push(bytes(pointer, len).to_vec());
}

#[unsafe(no_mangle)]
extern "C" fn qjs_host_log(_host: *mut c_void, level: u32, pointer: *const u8, len: usize) {
    let label = ["debug", "info", "warn", "error"].get(level as usize).copied().unwrap_or("log");
    println!("[{label}] {}", String::from_utf8_lossy(bytes(pointer, len)));
}

#[unsafe(no_mangle)]
extern "C" fn qjs_host_now_ms(host: *mut c_void) -> f64 {
    state(host).started.elapsed().as_secs_f64() * 1000.0
}

#[unsafe(no_mangle)]
extern "C" fn qjs_host_random_fill(pointer: *mut u8, len: usize) {
    // SAFETY: the glue checked the range against guest memory.
    let target = unsafe { std::slice::from_raw_parts_mut(pointer, len) };
    getrandom::fill(target).expect("random bytes");
}

#[unsafe(no_mangle)]
extern "C" fn qjs_host_timer_set(host: *mut c_void, id: u32, delay_ms: f64) {
    let due = Instant::now() + Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
    state(host).timers.insert(id, due);
}

#[unsafe(no_mangle)]
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
    fn boot(source: &[u8], echo: bool, memory_bytes: usize) -> Result<(Self, Duration), String> {
        let started = Instant::now();
        let host = Box::into_raw(Box::new(HostState {
            started: Instant::now(),
            timers: BTreeMap::new(),
            outbound: Vec::new(),
            echo,
        }));
        let mut instance = std::ptr::null_mut();
        // SAFETY: `host` stays alive for the guest's lifetime (see `Drop`).
        let created = unsafe { qjs_new(host.cast(), (memory_bytes / PAGE_BYTES) as u64, &mut instance) };
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

struct Options {
    script: String,
    memory_bytes: usize,
    native_stack_mib: usize,
    replay: Option<String>,
    instances: usize,
}

fn parse_options() -> Result<Options, String> {
    let usage = "usage: qjs-runner-wasm2c <script.js> [--memory-mib N] [--native-stack-mib N] [--replay corpus.bin] [--instances N]".to_string();
    let mut args = std::env::args().skip(1);
    let script = args.next().ok_or_else(|| usage.clone())?;
    let mut options =
        Options { script, memory_bytes: 64 << 20, native_stack_mib: 8, replay: None, instances: 0 };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| usage.clone())?;
        if flag == "--replay" {
            options.replay = Some(value);
            continue;
        }
        let number: usize = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
            "--instances" => options.instances = number,
            "--memory-mib" => options.memory_bytes = number << 20,
            "--native-stack-mib" => options.native_stack_mib = number,
            _ => return Err(usage),
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        },
    };
    let outcome = std::thread::Builder::new()
        .name("guest".into())
        .stack_size(options.native_stack_mib << 20)
        .spawn(move || run(&options))
        .expect("spawn guest thread")
        .join()
        .expect("guest thread panicked");
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[runner] guest stopped: {error}");
            ExitCode::FAILURE
        },
    }
}

fn run(options: &Options) -> Result<(), String> {
    let source = std::fs::read(&options.script).map_err(|error| format!("{}: {error}", options.script))?;
    let footprint_before = bench_common::footprint_bytes();
    // SAFETY: called once, on the thread that runs every guest.
    unsafe { qjs_runtime_init() };

    if options.instances > 0 {
        return measure_instances(&source, options, footprint_before);
    }
    if let Some(corpus) = &options.replay {
        let (mut guest, booted_in) = Guest::boot(&source, false, options.memory_bytes)?;
        return replay(&mut guest, corpus, booted_in);
    }

    let (mut guest, init_in) = Guest::boot(&source, true, options.memory_bytes)?;
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
    Ok(())
}

fn measure_instances(source: &[u8], options: &Options, footprint_before: u64) -> Result<(), String> {
    let count = options.instances;
    let mut guests = Vec::with_capacity(count);
    let mut boot_ms = Vec::with_capacity(count);
    let mut footprints = Vec::with_capacity(count);
    for _ in 0..count {
        let (guest, booted_in) = Guest::boot(source, false, options.memory_bytes)?;
        boot_ms.push(booted_in.as_secs_f64() * 1000.0);
        footprints.push(bench_common::footprint_bytes());
        guests.push(guest);
    }
    let first_mib = bench_common::mib(footprints[0] - footprint_before);
    let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
    let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
    println!(
        "RESULT {{\"engine\":\"{VARIANT}\",\"firstBootMs\":{:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"linearMemoryKiB\":{},\"instances\":{count}}}",
        boot_ms[0],
        guests[0].memory_kib(),
    );
    Ok(())
}

fn replay(guest: &mut Guest, corpus: &str, booted_in: Duration) -> Result<(), String> {
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
    println!(
        "RESULT {{\"engine\":\"{VARIANT}\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3},\"linearMemoryKiB\":{}}}",
        booted_in.as_secs_f64() * 1000.0,
        steps.len(),
        render_ms.len(),
        guest.memory_kib(),
    );
    if mismatches > 0 {
        return Err(format!("{mismatches} steps did not match the corpus"));
    }
    Ok(())
}
