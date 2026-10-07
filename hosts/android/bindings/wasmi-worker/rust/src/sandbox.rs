//! The wasmi sandbox around one guest instance.
//!
//! Each entry point is one bounded turn: the store is refuelled, the export is
//! called, and whatever the guest handed to `host.frame_send` and `host.log`
//! during that call is collected. A trap, fuel exhaustion or a memory grow
//! past the cap ends the turn with an error and poisons the instance: every
//! later turn answers [`Error::Poisoned`] without running guest code, so a
//! host that keeps feeding frames after a fault cannot resurrect a guest whose
//! memory it no longer trusts.
//!
//! Ported from `rust/crates/wasm-worker-probe/src/sandbox.rs` in the TrUAPI
//! repository, where the same sandbox runs the CLI proof.

use core::fmt;

use wasmi::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

/// Why a sandbox could not be built or a turn could not complete.
#[derive(Debug)]
pub enum Error {
    /// The module failed to compile, instantiate, or lacks a required export.
    Module(String),
    /// The guest trapped, ran out of fuel, or exceeded its memory cap.
    Trap {
        /// Fuel burned before the fault.
        fuel_burned: u64,
        /// wasmi's description of the fault.
        reason: String,
    },
    /// An earlier turn faulted; the instance accepts no more turns.
    Poisoned,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Module(reason) => write!(formatter, "module: {reason}"),
            Self::Trap {
                fuel_burned,
                reason,
            } => write!(
                formatter,
                "guest turn failed after {fuel_burned} fuel: {reason}"
            ),
            Self::Poisoned => write!(formatter, "sandbox is poisoned by an earlier fault"),
        }
    }
}

impl std::error::Error for Error {}

/// Result of a sandbox operation.
pub type Result<T> = core::result::Result<T, Error>;

fn module_error(context: &str, error: impl fmt::Display) -> Error {
    Error::Module(format!("{context}: {error}"))
}

/// Host-side state the imports write into.
struct HostState {
    frames: Vec<Vec<u8>>,
    logs: Vec<String>,
    limits: StoreLimits,
}

/// What to ask the guest to do this turn.
pub enum Turn {
    /// The connection opened.
    Start,
    /// One frame from the host.
    Frame(Vec<u8>),
    /// The worker is being paused.
    Suspend,
    /// The worker is being resumed.
    Resume,
}

/// What one turn produced.
#[derive(Debug)]
pub struct Outcome {
    /// Frames the guest sent, in order.
    pub frames: Vec<Vec<u8>>,
    /// Fuel the turn burned.
    pub fuel_burned: u64,
}

/// One instantiated guest.
pub struct Sandbox {
    store: Store<HostState>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    on_start: TypedFunc<(), ()>,
    on_frame: TypedFunc<(u32, u32), ()>,
    on_suspend: TypedFunc<(), ()>,
    on_resume: TypedFunc<(), ()>,
    fuel_per_turn: u64,
    fuel_burned: u64,
    turns: u64,
    poisoned: bool,
}

impl Sandbox {
    /// Instantiate `module` with `fuel_per_turn` per entry point and at most
    /// `memory_bytes` of linear memory.
    pub fn new(module: &[u8], fuel_per_turn: u64, memory_bytes: usize) -> Result<Self> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module =
            Module::new(&engine, module).map_err(|error| module_error("compile", error))?;
        let mut store = Store::new(
            &engine,
            HostState {
                frames: Vec::new(),
                logs: Vec::new(),
                limits: StoreLimitsBuilder::new().memory_size(memory_bytes).build(),
            },
        );
        store.limiter(|state| &mut state.limits);
        let mut linker = Linker::<HostState>::new(&engine);
        linker
            .func_wrap(
                "host",
                "frame_send",
                |mut caller: Caller<'_, HostState>,
                 ptr: u32,
                 len: u32|
                 -> core::result::Result<(), wasmi::Error> {
                    let bytes = read_guest(&mut caller, ptr, len)?;
                    caller.data_mut().frames.push(bytes);
                    Ok(())
                },
            )
            .map_err(|error| module_error("import host.frame_send", error))?;
        linker
            .func_wrap(
                "host",
                "log",
                |mut caller: Caller<'_, HostState>,
                 ptr: u32,
                 len: u32|
                 -> core::result::Result<(), wasmi::Error> {
                    let bytes = read_guest(&mut caller, ptr, len)?;
                    caller
                        .data_mut()
                        .logs
                        .push(String::from_utf8_lossy(&bytes).into_owned());
                    Ok(())
                },
            )
            .map_err(|error| module_error("import host.log", error))?;
        store
            .set_fuel(fuel_per_turn)
            .map_err(|error| module_error("fuel", error))?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .map_err(|error| module_error("instantiate", error))?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| Error::Module("module exports no memory".to_string()))?;
        fn export<Params, Results>(
            instance: &wasmi::Instance,
            store: &Store<HostState>,
            name: &str,
        ) -> Result<TypedFunc<Params, Results>>
        where
            Params: wasmi::WasmParams,
            Results: wasmi::WasmResults,
        {
            instance
                .get_func(store, name)
                .ok_or_else(|| Error::Module(format!("module exports no `{name}`")))?
                .typed(store)
                .map_err(|error| module_error(&format!("export `{name}`"), error))
        }
        Ok(Self {
            alloc: export(&instance, &store, "alloc")?,
            free: export(&instance, &store, "free")?,
            on_start: export(&instance, &store, "on_start")?,
            on_frame: export(&instance, &store, "on_frame")?,
            on_suspend: export(&instance, &store, "on_suspend")?,
            on_resume: export(&instance, &store, "on_resume")?,
            store,
            memory,
            fuel_per_turn,
            fuel_burned: 0,
            turns: 0,
            poisoned: false,
        })
    }

    /// Run one turn. A fault poisons the sandbox for every later turn.
    pub fn run(&mut self, turn: Turn) -> Result<Outcome> {
        if self.poisoned {
            return Err(Error::Poisoned);
        }
        let outcome = self.run_unguarded(turn);
        if outcome.is_err() {
            self.poisoned = true;
        }
        outcome
    }

    /// Whether an earlier turn faulted.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn run_unguarded(&mut self, turn: Turn) -> Result<Outcome> {
        self.store
            .set_fuel(self.fuel_per_turn)
            .map_err(|error| module_error("fuel", error))?;
        let called = match turn {
            Turn::Start => self.on_start.call(&mut self.store, ()),
            Turn::Suspend => self.on_suspend.call(&mut self.store, ()),
            Turn::Resume => self.on_resume.call(&mut self.store, ()),
            Turn::Frame(bytes) => self.frame_turn(&bytes),
        };
        let remaining = self.store.get_fuel().unwrap_or(0);
        let burned = self.fuel_per_turn.saturating_sub(remaining);
        self.fuel_burned += burned;
        self.turns += 1;
        called.map_err(|error| Error::Trap {
            fuel_burned: burned,
            reason: error.to_string(),
        })?;
        Ok(Outcome {
            frames: core::mem::take(&mut self.store.data_mut().frames),
            fuel_burned: burned,
        })
    }

    fn frame_turn(&mut self, bytes: &[u8]) -> core::result::Result<(), wasmi::Error> {
        let len =
            u32::try_from(bytes.len()).map_err(|_| wasmi::Error::new("frame longer than u32"))?;
        let ptr = self.alloc.call(&mut self.store, len)?;
        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .map_err(|error| wasmi::Error::new(format!("copy frame into guest memory: {error}")))?;
        let called = self.on_frame.call(&mut self.store, (ptr, len));
        let freed = self.free.call(&mut self.store, (ptr, len));
        called.and(freed)
    }

    /// Lines the guest logged since the last call.
    pub fn take_logs(&mut self) -> Vec<String> {
        core::mem::take(&mut self.store.data_mut().logs)
    }

    /// Turns run so far.
    pub fn turns(&self) -> u64 {
        self.turns
    }

    /// Fuel burned across every turn.
    pub fn fuel_burned(&self) -> u64 {
        self.fuel_burned
    }

    /// Current size of the guest's linear memory.
    pub fn memory_bytes(&self) -> usize {
        self.memory.data(&self.store).len()
    }
}

fn read_guest(
    caller: &mut Caller<'_, HostState>,
    ptr: u32,
    len: u32,
) -> core::result::Result<Vec<u8>, wasmi::Error> {
    let memory = caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .ok_or_else(|| wasmi::Error::new("guest exports no memory"))?;
    let mut bytes = vec![0u8; len as usize];
    memory
        .read(&*caller, ptr as usize, &mut bytes)
        .map_err(|error| wasmi::Error::new(format!("guest pointer out of bounds: {error}")))?;
    Ok(bytes)
}
