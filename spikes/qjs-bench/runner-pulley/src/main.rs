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
}

struct Options {
    guest: String,
    script: String,
    fuel_per_turn: u64,
    memory_bytes: usize,
    native: bool,
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
    let mut options =
        Options { guest, script, fuel_per_turn: 2_000_000_000, memory_bytes: 64 << 20, native: false };
    while let Some(flag) = args.next() {
        if flag == "--native" {
            options.native = true;
            continue;
        }
        let value = args.next().ok_or_else(|| usage.clone())?;
        let number: u64 = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
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

    let compile_started = Instant::now();
    let module = Module::new(&engine, &wasm)?;
    let compiled_in = compile_started.elapsed();

    let state = HostState {
        started: Instant::now(),
        limits: StoreLimitsBuilder::new().memory_size(options.memory_bytes).build(),
        timers: BTreeMap::new(),
        outbound: Vec::new(),
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|state| &mut state.limits);

    let mut linker = Linker::<HostState>::new(&engine);
    link_truapi(&mut linker)?;
    link_wasi(&mut linker, &module)?;

    let instantiate_started = Instant::now();
    store.set_fuel(options.fuel_per_turn)?;
    let instance = linker.instantiate(&mut store, &module)?;
    let instantiated_in = instantiate_started.elapsed();
    let memory = instance.get_memory(&mut store, "memory").ok_or_else(|| anyhow!("guest exports no memory"))?;

    let mut guest = Guest { store, instance, memory, fuel_per_turn: options.fuel_per_turn };
    let init_started = Instant::now();
    let status = guest.init(&source)?;
    guest.settle()?;
    let init_in = init_started.elapsed();
    let backend = if options.native { "cranelift native" } else { "pulley" };
    eprintln!(
        "[pulley] {backend}: compile {compiled_in:?}, instantiate {instantiated_in:?}, init to idle {init_in:?}, status {status}"
    );
    if status != 0 {
        bail!("init returned {status}");
    }

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
