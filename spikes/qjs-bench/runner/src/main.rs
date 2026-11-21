//! M0 T1 check: run a script in the QuickJS guest under wasmi on macOS.
//!
//! Usage: qjs-runner <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N]
//!
//! Every outbound frame is echoed back as an inbound frame. Timers run on
//! wall-clock time. The process exits non-zero if the guest traps.

use std::{
    collections::BTreeMap,
    process::ExitCode,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use wasmi::{
    Caller, Config, Engine, Error, ExternType, Instance, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder, TypedFunc, Val,
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
    native_stack_mib: usize,
}

fn main() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        },
    };
    let stack_bytes = options.native_stack_mib << 20;
    let outcome = std::thread::Builder::new()
        .name("guest".into())
        .stack_size(stack_bytes)
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

fn parse_options() -> Result<Options, String> {
    let usage = "usage: qjs-runner <guest.wasm> <script.js> [--fuel-per-turn N] [--memory-mib N] [--native-stack-mib N]"
        .to_string();
    let mut args = std::env::args().skip(1);
    let guest = args.next().ok_or_else(|| usage.clone())?;
    let script = args.next().ok_or_else(|| usage.clone())?;
    let mut options =
        Options { guest, script, fuel_per_turn: 2_000_000_000, memory_bytes: 64 << 20, native_stack_mib: 8 };
    while let Some(flag) = args.next() {
        let value = args.next().ok_or_else(|| usage.clone())?;
        let number: u64 = value.parse().map_err(|_| usage.clone())?;
        match flag.as_str() {
            "--fuel-per-turn" => options.fuel_per_turn = number,
            "--memory-mib" => options.memory_bytes = (number as usize) << 20,
            "--native-stack-mib" => options.native_stack_mib = number as usize,
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

    /// Run jobs and echo frames until nothing is left to do right now.
    fn settle(&mut self) -> Result<(), Error> {
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

fn run(options: &Options) -> Result<(), Error> {
    let wasm = std::fs::read(&options.guest).map_err(|error| Error::new(format!("{}: {error}", options.guest)))?;
    let source = std::fs::read(&options.script).map_err(|error| Error::new(format!("{}: {error}", options.script)))?;

    let mut config = Config::default();
    config.consume_fuel(options.fuel_per_turn > 0);
    if std::env::var_os("QJS_EAGER").is_some() {
        config.compilation_mode(wasmi::CompilationMode::Eager);
    }
    let engine = Engine::new(&config);

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
    if options.fuel_per_turn > 0 {
        store.set_fuel(options.fuel_per_turn)?;
    }
    let instance = linker.instantiate_and_start(&mut store, &module)?;
    let instantiated_in = instantiate_started.elapsed();
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| Error::new("guest exports no memory"))?;

    let mut guest = Guest { store, instance, memory, fuel_per_turn: options.fuel_per_turn };
    let init_started = Instant::now();
    let status = guest.init(&source)?;
    guest.settle()?;
    let init_in = init_started.elapsed();
    eprintln!(
        "[runner] compile {compiled_in:?}, instantiate {instantiated_in:?}, init to idle {init_in:?}, status {status}"
    );
    if status != 0 {
        return Err(Error::new(format!("init returned {status}")));
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
    let pages = guest.memory.size(&guest.store);
    eprintln!("[runner] idle, no timers left; linear memory {} KiB", pages * 64);
    Ok(())
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
        let label = ["debug", "info", "warn", "error"].get(level as usize).copied().unwrap_or("log");
        println!("[{label}] {}", String::from_utf8_lossy(&message));
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
