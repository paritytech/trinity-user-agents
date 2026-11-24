//! The QuickJS guest under wasmi: replay, instances and run-to-completion
//! measurements, shared by the `qjs-runner` binary and the iOS bench.
//!
//! Every outbound frame is echoed back as an inbound frame when a run
//! echoes. Timers run on wall-clock time. A guest trap is an error.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub use wasmi::Error;
use wasmi::{
    Caller, Config, Engine, ExternType, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc, Val,
};

/// What every guest a [`Limits`] run boots is allowed.
pub struct Limits {
    /// Fuel per call into the guest; 0 turns metering off.
    pub fuel_per_turn: u64,
    /// The guest's linear memory cap.
    pub memory_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { fuel_per_turn: 2_000_000_000, memory_bytes: 64 << 20 }
    }
}

const WASI: &str = "wasi_snapshot_preview1";
const ERRNO_SUCCESS: i32 = 0;
const ERRNO_BADF: i32 = 8;
const ERRNO_INVAL: i32 = 28;

struct HostState {
    started: Instant,
    limits: StoreLimits,
    timers: BTreeMap<u32, Instant>,
    outbound: Vec<Vec<u8>>,
    /// Echo outbound frames back to the guest (ping-pong); off for replay.
    echo: bool,
}

struct Guest {
    store: Store<HostState>,
    instance: Instance,
    memory: Memory,
    fuel_per_turn: u64,
}

impl Guest {
    fn export<P: wasmi::WasmParams, R: wasmi::WasmResults>(&self, name: &str) -> Result<TypedFunc<P, R>, Error> {
        self.instance.get_typed_func::<P, R>(&self.store, name)
    }

    /// `--fuel-per-turn 0` turns metering off, to measure what it costs.
    fn refuel(&mut self) -> Result<(), Error> {
        if self.fuel_per_turn == 0 {
            return Ok(());
        }
        self.store.set_fuel(self.fuel_per_turn)
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<i32, Error> {
        let alloc = self.export::<i32, i32>("alloc")?;
        self.refuel()?;
        let pointer = alloc.call(&mut self.store, bytes.len() as i32)?;
        self.memory.write(&mut self.store, pointer as usize, bytes).map_err(|error| Error::new(error.to_string()))?;
        Ok(pointer)
    }

    fn free_bytes(&mut self, pointer: i32, len: usize) -> Result<(), Error> {
        let dealloc = self.export::<(i32, i32), ()>("dealloc")?;
        dealloc.call(&mut self.store, (pointer, len as i32))
    }

    fn init(&mut self, source: &[u8]) -> Result<i32, Error> {
        let pointer = self.write_bytes(source)?;
        let init = self.export::<(i32, i32), i32>("init")?;
        self.refuel()?;
        let status = init.call(&mut self.store, (pointer, source.len() as i32))?;
        self.free_bytes(pointer, source.len())?;
        Ok(status)
    }

    fn deliver(&mut self, frame: &[u8]) -> Result<(), Error> {
        let pointer = self.write_bytes(frame)?;
        let on_frame = self.export::<(i32, i32), ()>("on_frame")?;
        self.refuel()?;
        on_frame.call(&mut self.store, (pointer, frame.len() as i32))?;
        self.free_bytes(pointer, frame.len())
    }

    fn fire_timer(&mut self, id: u32) -> Result<(), Error> {
        let on_timer = self.export::<i32, ()>("on_timer")?;
        self.refuel()?;
        on_timer.call(&mut self.store, id as i32)
    }

    fn run_jobs(&mut self) -> Result<i32, Error> {
        let run_jobs = self.export::<(), i32>("run_jobs")?;
        self.refuel()?;
        run_jobs.call(&mut self.store, ())
    }

    /// Run jobs (and echo frames, if echoing) until nothing is left to do
    /// right now. Without echo, outbound frames stay queued for the caller.
    fn settle(&mut self) -> Result<(), Error> {
        loop {
            self.run_jobs()?;
            if !self.store.data().echo {
                return Ok(());
            }
            let outbound = std::mem::take(&mut self.store.data_mut().outbound);
            if outbound.is_empty() {
                return Ok(());
            }
            for frame in outbound {
                self.deliver(&frame)?;
            }
        }
    }
}

/// One compiled module plus a linker, from which any number of guests boot.
struct Host {
    engine: Engine,
    module: Module,
    linker: Linker<HostState>,
    fuel_per_turn: u64,
    memory_bytes: usize,
}

impl Host {
    /// Instantiate a fresh guest and evaluate `source` until it is idle.
    /// Returns the guest and the time it took, from instantiation to idle.
    fn boot(&self, source: &[u8], echo: bool) -> Result<(Guest, Duration), Error> {
        let started = Instant::now();
        let state = HostState {
            started: Instant::now(),
            limits: StoreLimitsBuilder::new().memory_size(self.memory_bytes).build(),
            timers: BTreeMap::new(),
            outbound: Vec::new(),
            echo,
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        if self.fuel_per_turn > 0 {
            store.set_fuel(self.fuel_per_turn)?;
        }
        let instance = self.linker.instantiate_and_start(&mut store, &self.module)?;
        let memory = instance.get_memory(&store, "memory").ok_or_else(|| Error::new("guest exports no memory"))?;
        let mut guest = Guest { store, instance, memory, fuel_per_turn: self.fuel_per_turn };
        let status = guest.init(source)?;
        guest.settle()?;
        if status != 0 {
            return Err(Error::new(format!("init returned {status}")));
        }
        Ok((guest, started.elapsed()))
    }
}

/// Compile `wasm` and link the host imports. Returns the host and how long
/// the compile took.
fn compile(wasm: &[u8], limits: &Limits) -> Result<(Host, Duration), Error> {
    let mut config = Config::default();
    config.consume_fuel(limits.fuel_per_turn > 0);
    if std::env::var_os("QJS_EAGER").is_some() {
        config.compilation_mode(wasmi::CompilationMode::Eager);
    }
    let engine = Engine::new(&config);

    let compile_started = Instant::now();
    let module = Module::new(&engine, wasm)?;
    let compiled_in = compile_started.elapsed();

    let mut linker = Linker::<HostState>::new(&engine);
    link_truapi(&mut linker)?;
    link_wasi(&mut linker, &module)?;
    let host =
        Host { engine, module, linker, fuel_per_turn: limits.fuel_per_turn, memory_bytes: limits.memory_bytes };
    Ok((host, compiled_in))
}

/// Run `source` to completion with frames echoed back: evaluate, drain
/// jobs, fire timers on wall-clock time. Returns the time from instantiation
/// to idle.
pub fn run_to_completion(wasm: &[u8], source: &[u8], limits: &Limits) -> Result<Duration, Error> {
    let (host, compiled_in) = compile(wasm, limits)?;
    let (mut guest, init_in) = host.boot(source, true)?;
    eprintln!("[runner] compile {compiled_in:?}, instantiate and init to idle {init_in:?}");

    while let Some((id, due)) = next_timer(&guest.store) {
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        guest.store.data_mut().timers.remove(&id);
        guest.fire_timer(id)?;
        guest.settle()?;
    }
    let pages = guest.memory.size(&guest.store);
    eprintln!("[runner] idle, no timers left; linear memory {} KiB", pages * 64);
    Ok(init_in)
}

/// Boot `count` guests on the worker bundle, keeping each alive, and report
/// cold start and the memory each one adds to the process.
pub fn instances(wasm: &[u8], source: &[u8], count: usize, limits: &Limits) -> Result<String, Error> {
    let footprint_before = bench_common::footprint_bytes();
    let (host, compiled_in) = compile(wasm, limits)?;
    let after_compile = bench_common::footprint_bytes();
    let mut guests = Vec::with_capacity(count);
    let mut boot_ms = Vec::with_capacity(count);
    let mut footprints = Vec::with_capacity(count);
    for _ in 0..count {
        let (guest, booted_in) = host.boot(source, false)?;
        boot_ms.push(booted_in.as_secs_f64() * 1000.0);
        footprints.push(bench_common::footprint_bytes());
        guests.push(guest);
    }
    let first_ms = boot_ms[0];
    let first_mib = bench_common::mib(footprints[0] - after_compile);
    let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
    let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
    let linear_kib = guests[0].memory.size(&guests[0].store) * 64;
    Ok(format!(
        "{{\"engine\":\"wasmi\",\"compileMs\":{:.1},\"moduleMiB\":{:.2},\"firstBootMs\":{first_ms:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"linearMemoryKiB\":{linear_kib},\"instances\":{count}}}",
        compiled_in.as_secs_f64() * 1000.0,
        bench_common::mib(after_compile - footprint_before),
    ))
}

/// Replay the corpus against a fresh guest, checking every outbound frame
/// byte for byte and timing each render request until its tree comes back.
pub fn replay(wasm: &[u8], source: &[u8], corpus: &str, limits: &Limits) -> Result<String, Error> {
    let (host, _) = compile(wasm, limits)?;
    let (mut guest, booted_in) = host.boot(source, false)?;
    let guest = &mut guest;
    let steps = bench_common::load_corpus(corpus);
    let mut render_ms = Vec::new();
    let mut mismatches = 0;
    for step in &steps {
        let started = Instant::now();
        guest.deliver(&step.inbound)?;
        guest.settle()?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        let sent = std::mem::take(&mut guest.store.data_mut().outbound);
        if sent != step.outbound {
            mismatches += 1;
        }
        if bench_common::is_render_start(&step.inbound) {
            render_ms.push(elapsed);
        }
    }
    let (p50, p95) = bench_common::percentiles(&mut render_ms);
    Ok(format!(
        "{{\"engine\":\"wasmi\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3},\"linearMemoryKiB\":{}}}",
        booted_in.as_secs_f64() * 1000.0,
        steps.len(),
        render_ms.len(),
        guest.memory.size(&guest.store) * 64,
    ))
}

fn next_timer(store: &Store<HostState>) -> Option<(u32, Instant)> {
    store.data().timers.iter().min_by_key(|(_, due)| **due).map(|(id, due)| (*id, *due))
}

fn guest_memory(caller: &Caller<'_, HostState>) -> Result<Memory, Error> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| Error::new("guest exports no memory"))
}

fn read_guest(caller: &Caller<'_, HostState>, pointer: i32, len: i32) -> Result<Vec<u8>, Error> {
    let memory = guest_memory(caller)?;
    let mut bytes = vec![0; len as usize];
    memory.read(caller, pointer as usize, &mut bytes).map_err(|error| Error::new(error.to_string()))?;
    Ok(bytes)
}

fn write_guest(caller: &mut Caller<'_, HostState>, pointer: i32, bytes: &[u8]) -> Result<(), Error> {
    let memory = guest_memory(caller)?;
    memory.write(caller, pointer as usize, bytes).map_err(|error| Error::new(error.to_string()))
}

fn link_truapi(linker: &mut Linker<HostState>) -> Result<(), Error> {
    linker.func_wrap("truapi", "frame_out", |caller: Caller<'_, HostState>, pointer: i32, len: i32| {
        let frame = read_guest(&caller, pointer, len)?;
        let mut caller = caller;
        caller.data_mut().outbound.push(frame);
        Ok(())
    })?;
    linker.func_wrap("truapi", "now_ms", |caller: Caller<'_, HostState>| {
        caller.data().started.elapsed().as_secs_f64() * 1000.0
    })?;
    linker.func_wrap("truapi", "random_fill", |mut caller: Caller<'_, HostState>, pointer: i32, len: i32| {
        let mut bytes = vec![0; len as usize];
        getrandom::fill(&mut bytes).map_err(|error| Error::new(error.to_string()))?;
        write_guest(&mut caller, pointer, &bytes)
    })?;
    linker.func_wrap("truapi", "timer_set", |mut caller: Caller<'_, HostState>, id: i32, delay_ms: f64| {
        let due = Instant::now() + Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
        caller.data_mut().timers.insert(id as u32, due);
    })?;
    linker.func_wrap("truapi", "timer_clear", |mut caller: Caller<'_, HostState>, id: i32| {
        caller.data_mut().timers.remove(&(id as u32));
    })?;
    linker.func_wrap("truapi", "log", |caller: Caller<'_, HostState>, level: i32, pointer: i32, len: i32| {
        let message = read_guest(&caller, pointer, len)?;
        bench_common::log_line(level as u32, &String::from_utf8_lossy(&message));
        Ok(())
    })?;
    Ok(())
}

/// Define every WASI import the module declares. A small set gets a real,
/// harmless implementation; anything else traps with its name when called.
fn link_wasi(linker: &mut Linker<HostState>, module: &Module) -> Result<(), Error> {
    for import in module.imports() {
        if import.module() != WASI {
            continue;
        }
        let ExternType::Func(func_type) = import.ty().clone() else {
            return Err(Error::new(format!("unexpected WASI import kind: {}", import.name())));
        };
        let name = import.name().to_string();
        eprintln!("[runner] WASI import: {name}");
        linker.func_new(WASI, &name.clone(), func_type, move |mut caller, params, results| {
            let errno = wasi_call(&mut caller, &name, params)?;
            if let Some(result) = results.first_mut() {
                *result = Val::I32(errno);
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn arg_i32(params: &[Val], index: usize) -> i32 {
    match params.get(index) {
        Some(Val::I32(value)) => *value,
        _ => 0,
    }
}

fn wasi_call(caller: &mut Caller<'_, HostState>, name: &str, params: &[Val]) -> Result<i32, Error> {
    match name {
        "fd_write" => {
            let (fd, iovs, iovs_len, written_out) =
                (arg_i32(params, 0), arg_i32(params, 1), arg_i32(params, 2), arg_i32(params, 3));
            if fd != 1 && fd != 2 {
                return Ok(ERRNO_BADF);
            }
            let mut written = 0u32;
            for index in 0..iovs_len {
                let entry = read_guest(caller, iovs + index * 8, 8)?;
                let pointer = i32::from_le_bytes(entry[0..4].try_into().unwrap());
                let len = i32::from_le_bytes(entry[4..8].try_into().unwrap());
                let bytes = read_guest(caller, pointer, len)?;
                eprint!("[guest fd{fd}] {}", String::from_utf8_lossy(&bytes));
                written += len as u32;
            }
            write_guest(caller, written_out, &written.to_le_bytes())?;
            Ok(ERRNO_SUCCESS)
        },
        "clock_time_get" => {
            let (clock_id, time_out) = (arg_i32(params, 0), arg_i32(params, 2));
            let nanos: u64 = match clock_id {
                0 => SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64,
                1 => caller.data().started.elapsed().as_nanos() as u64,
                _ => return Ok(ERRNO_INVAL),
            };
            write_guest(caller, time_out, &nanos.to_le_bytes())?;
            Ok(ERRNO_SUCCESS)
        },
        "random_get" => {
            let (pointer, len) = (arg_i32(params, 0), arg_i32(params, 1));
            let mut bytes = vec![0; len as usize];
            getrandom::fill(&mut bytes).map_err(|error| Error::new(error.to_string()))?;
            write_guest(caller, pointer, &bytes)?;
            Ok(ERRNO_SUCCESS)
        },
        "environ_sizes_get" | "args_sizes_get" => {
            let (count_out, size_out) = (arg_i32(params, 0), arg_i32(params, 1));
            write_guest(caller, count_out, &0u32.to_le_bytes())?;
            write_guest(caller, size_out, &0u32.to_le_bytes())?;
            Ok(ERRNO_SUCCESS)
        },
        "environ_get" | "args_get" => Ok(ERRNO_SUCCESS),
        "proc_exit" => Err(Error::i32_exit(arg_i32(params, 0))),
        other => Err(Error::new(format!("guest called WASI {other}, which this host does not provide"))),
    }
}
