//! M0 T5: the same guest under wasmtime's Pulley interpreter (E4).
//!
//! Usage: qjs-runner-pulley <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N]
//!
//! Mirrors `qjs-runner`: same `truapi` imports, same restricted WASI set,
//! fuel per turn, a linear-memory cap, frames echoed back, timers on
//! wall-clock time. `--native` compiles for the host CPU with Cranelift
//! instead of Pulley, which is what Android could do.

use std::{
    collections::BTreeMap,
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use wasmtime::{bail, error::Context, format_err as anyhow, Result};
use wasmtime::{
    Caller, Config, Engine, ExternType, Instance, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc, Val, WasmParams, WasmResults,
};

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

struct Options {
    guest: String,
    script: String,
    fuel_per_turn: u64,
    memory_bytes: usize,
    native: bool,
    replay: Option<String>,
    instances: usize,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        },
    };
    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[pulley] guest stopped: {error:#}");
            ExitCode::FAILURE
        },
    }
}

fn parse_options() -> Result<Options, String> {
    let usage =
        "usage: qjs-runner-pulley <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N] [--native]".to_string();
    let mut args = std::env::args().skip(1);
    let guest = args.next().ok_or_else(|| usage.clone())?;
    let script = args.next().ok_or_else(|| usage.clone())?;
    let mut options = Options {
        guest,
        script,
        fuel_per_turn: 2_000_000_000,
        memory_bytes: 64 << 20,
        native: false,
        replay: None,
        instances: 0,
    };
    while let Some(flag) = args.next() {
        if flag == "--native" {
            options.native = true;
            continue;
        }
        let value = args.next().ok_or_else(|| usage.clone())?;
        if flag == "--replay" {
            options.replay = Some(value);
            continue;
        }
        let number: u64 = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
            "--instances" => options.instances = number as usize,
            "--fuel-per-turn" => options.fuel_per_turn = number,
            "--memory-mib" => options.memory_bytes = (number as usize) << 20,
            _ => return Err(usage),
        }
    }
    Ok(options)
}

struct Guest {
    store: Store<HostState>,
    instance: Instance,
    memory: Memory,
    fuel_per_turn: u64,
}

impl Guest {
    fn export<P: WasmParams, R: WasmResults>(&mut self, name: &str) -> Result<TypedFunc<P, R>> {
        self.instance.get_typed_func::<P, R>(&mut self.store, name)
    }

    fn refuel(&mut self) -> Result<()> {
        self.store.set_fuel(self.fuel_per_turn)
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<i32> {
        let alloc = self.export::<i32, i32>("alloc")?;
        self.refuel()?;
        let pointer = alloc.call(&mut self.store, bytes.len() as i32)?;
        self.memory.write(&mut self.store, pointer as usize, bytes)?;
        Ok(pointer)
    }

    fn free_bytes(&mut self, pointer: i32, len: usize) -> Result<()> {
        let dealloc = self.export::<(i32, i32), ()>("dealloc")?;
        dealloc.call(&mut self.store, (pointer, len as i32))
    }

    fn init(&mut self, source: &[u8]) -> Result<i32> {
        let pointer = self.write_bytes(source)?;
        let init = self.export::<(i32, i32), i32>("init")?;
        self.refuel()?;
        let status = init.call(&mut self.store, (pointer, source.len() as i32))?;
        self.free_bytes(pointer, source.len())?;
        Ok(status)
    }

    fn deliver(&mut self, frame: &[u8]) -> Result<()> {
        let pointer = self.write_bytes(frame)?;
        let on_frame = self.export::<(i32, i32), ()>("on_frame")?;
        self.refuel()?;
        on_frame.call(&mut self.store, (pointer, frame.len() as i32))?;
        self.free_bytes(pointer, frame.len())
    }

    fn fire_timer(&mut self, id: u32) -> Result<()> {
        let on_timer = self.export::<i32, ()>("on_timer")?;
        self.refuel()?;
        on_timer.call(&mut self.store, id as i32)
    }

    fn run_jobs(&mut self) -> Result<i32> {
        let run_jobs = self.export::<(), i32>("run_jobs")?;
        self.refuel()?;
        run_jobs.call(&mut self.store, ())
    }

    fn settle(&mut self) -> Result<()> {
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

fn run(options: &Options) -> Result<()> {
    let wasm = std::fs::read(&options.guest).with_context(|| options.guest.clone())?;
    let source = std::fs::read(&options.script).with_context(|| options.script.clone())?;

    let mut config = Config::new();
    config.consume_fuel(true);
    if !options.native {
        config.target("pulley64")?;
    }
    let engine = Engine::new(&config)?;

    let footprint_before = bench_common::footprint_bytes();
    let compile_started = Instant::now();
    let module = Module::new(&engine, &wasm)?;
    let compiled_in = compile_started.elapsed();

    let mut linker = Linker::<HostState>::new(&engine);
    link_truapi(&mut linker)?;
    link_wasi(&mut linker, &module)?;
    let backend = if options.native { "cranelift" } else { "pulley" };
    let host = Host { engine, module, linker, fuel_per_turn: options.fuel_per_turn, memory_bytes: options.memory_bytes };

    if options.instances > 0 {
        let after_compile = bench_common::footprint_bytes();
        let mut guests = Vec::new();
        let mut boot_ms = Vec::new();
        let mut footprints = Vec::new();
        for _ in 0..options.instances {
            let (guest, booted_in) = host.boot(&source, false)?;
            boot_ms.push(booted_in.as_secs_f64() * 1000.0);
            footprints.push(bench_common::footprint_bytes());
            guests.push(guest);
        }
        let count = options.instances;
        let first_mib = bench_common::mib(footprints[0] - after_compile);
        let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
        let first_ms = boot_ms[0];
        let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
        let linear_kib = guests[0].memory.data_size(&guests[0].store) / 1024;
        println!(
            "RESULT {{\"engine\":\"{backend}\",\"compileMs\":{:.1},\"moduleMiB\":{:.2},\"firstBootMs\":{first_ms:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"linearMemoryKiB\":{linear_kib},\"instances\":{count}}}",
            compiled_in.as_secs_f64() * 1000.0,
            bench_common::mib(after_compile - footprint_before),
        );
        return Ok(());
    }

    if let Some(corpus) = &options.replay {
        let (mut guest, booted_in) = host.boot(&source, false)?;
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
        println!(
            "RESULT {{\"engine\":\"{backend}\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3}}}",
            booted_in.as_secs_f64() * 1000.0,
            steps.len(),
            render_ms.len(),
        );
        if mismatches > 0 {
            bail!("{mismatches} steps did not match the corpus");
        }
        return Ok(());
    }

    let (mut guest, init_in) = host.boot(&source, true)?;
    eprintln!("[pulley] {backend}: compile {compiled_in:?}, instantiate and init to idle {init_in:?}");

    while let Some((id, due)) = next_timer(&guest.store) {
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        guest.store.data_mut().timers.remove(&id);
        guest.fire_timer(id)?;
        guest.settle()?;
    }
    let bytes = guest.memory.data_size(&guest.store);
    eprintln!("[pulley] idle, no timers left; linear memory {} KiB", bytes / 1024);
    Ok(())
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
    fn boot(&self, source: &[u8], echo: bool) -> Result<(Guest, Duration)> {
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
        store.set_fuel(self.fuel_per_turn)?;
        let instance = self.linker.instantiate(&mut store, &self.module)?;
        let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow!("guest exports no memory"))?;
        let mut guest = Guest { store, instance, memory, fuel_per_turn: self.fuel_per_turn };
        let status = guest.init(source)?;
        guest.settle()?;
        if status != 0 {
            bail!("init returned {status}");
        }
        Ok((guest, started.elapsed()))
    }
}

fn next_timer(store: &Store<HostState>) -> Option<(u32, Instant)> {
    store.data().timers.iter().min_by_key(|(_, due)| **due).map(|(id, due)| (*id, *due))
}

fn guest_memory(caller: &mut Caller<'_, HostState>) -> Result<Memory> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| anyhow!("guest exports no memory"))
}

fn read_guest(caller: &mut Caller<'_, HostState>, pointer: i32, len: i32) -> Result<Vec<u8>> {
    let memory = guest_memory(caller)?;
    let mut bytes = vec![0; len as usize];
    memory.read(&*caller, pointer as usize, &mut bytes)?;
    Ok(bytes)
}

fn write_guest(caller: &mut Caller<'_, HostState>, pointer: i32, bytes: &[u8]) -> Result<()> {
    let memory = guest_memory(caller)?;
    memory.write(caller, pointer as usize, bytes)?;
    Ok(())
}

fn link_truapi(linker: &mut Linker<HostState>) -> Result<()> {
    linker.func_wrap("truapi", "frame_out", |mut caller: Caller<'_, HostState>, pointer: i32, len: i32| {
        let frame = read_guest(&mut caller, pointer, len)?;
        caller.data_mut().outbound.push(frame);
        Ok(())
    })?;
    linker.func_wrap("truapi", "now_ms", |caller: Caller<'_, HostState>| {
        caller.data().started.elapsed().as_secs_f64() * 1000.0
    })?;
    linker.func_wrap("truapi", "random_fill", |mut caller: Caller<'_, HostState>, pointer: i32, len: i32| {
        let mut bytes = vec![0; len as usize];
        getrandom::fill(&mut bytes).map_err(|error| anyhow!("{error}"))?;
        write_guest(&mut caller, pointer, &bytes)
    })?;
    linker.func_wrap("truapi", "timer_set", |mut caller: Caller<'_, HostState>, id: i32, delay_ms: f64| {
        let due = Instant::now() + Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
        caller.data_mut().timers.insert(id as u32, due);
    })?;
    linker.func_wrap("truapi", "timer_clear", |mut caller: Caller<'_, HostState>, id: i32| {
        caller.data_mut().timers.remove(&(id as u32));
    })?;
    linker.func_wrap("truapi", "log", |mut caller: Caller<'_, HostState>, level: i32, pointer: i32, len: i32| {
        let message = read_guest(&mut caller, pointer, len)?;
        let label = ["debug", "info", "warn", "error"].get(level as usize).copied().unwrap_or("log");
        println!("[{label}] {}", String::from_utf8_lossy(&message));
        Ok(())
    })?;
    Ok(())
}

fn link_wasi(linker: &mut Linker<HostState>, module: &Module) -> Result<()> {
    for import in module.imports() {
        if import.module() != WASI {
            continue;
        }
        let ExternType::Func(func_type) = import.ty() else {
            bail!("unexpected WASI import kind: {}", import.name());
        };
        let name = import.name().to_string();
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
    params.get(index).and_then(Val::i32).unwrap_or(0)
}

fn wasi_call(caller: &mut Caller<'_, HostState>, name: &str, params: &[Val]) -> Result<i32> {
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
                let pointer = i32::from_le_bytes(entry[0..4].try_into()?);
                let len = i32::from_le_bytes(entry[4..8].try_into()?);
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
            getrandom::fill(&mut bytes).map_err(|error| anyhow!("{error}"))?;
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
        "proc_exit" => bail!("guest called proc_exit({})", arg_i32(params, 0)),
        other => bail!("guest called WASI {other}, which this host does not provide"),
    }
}
