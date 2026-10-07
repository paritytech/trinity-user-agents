//! The wasmi sandbox around one guest instance.
//!
//! Each entry point is one bounded turn: the store is refuelled, the export is
//! called, and whatever the guest handed to `host.frame_send` and `host.log`
//! during that call is collected. A trap, fuel exhaustion or a memory grow
//! past the cap ends the turn with an error and leaves the instance unusable,
//! which the caller reports and stops on.

use anyhow::{Context, Result, anyhow};
use wasmi::{
    Caller, Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

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
}

impl Sandbox {
    /// Instantiate `module` with `fuel_per_turn` per entry point and at most
    /// `memory_bytes` of linear memory.
    pub fn new(module: &[u8], fuel_per_turn: u64, memory_bytes: usize) -> Result<Self> {
        let mut config = Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, module).context("compile module")?;
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
        linker.func_wrap(
            "host",
            "frame_send",
            |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> Result<(), wasmi::Error> {
                let bytes = read_guest(&mut caller, ptr, len)?;
                caller.data_mut().frames.push(bytes);
                Ok(())
            },
        )?;
        linker.func_wrap(
            "host",
            "log",
            |mut caller: Caller<'_, HostState>, ptr: u32, len: u32| -> Result<(), wasmi::Error> {
                let bytes = read_guest(&mut caller, ptr, len)?;
                caller
                    .data_mut()
                    .logs
                    .push(String::from_utf8_lossy(&bytes).into_owned());
                Ok(())
            },
        )?;
        store.set_fuel(fuel_per_turn)?;
        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .context("instantiate module")?;
        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| anyhow!("module exports no memory"))?;
        let export = |name: &str| {
            instance
                .get_func(&store, name)
                .ok_or_else(|| anyhow!("module exports no `{name}`"))
        };
        Ok(Self {
            alloc: export("alloc")?.typed(&store)?,
            free: export("free")?.typed(&store)?,
            on_start: export("on_start")?.typed(&store)?,
            on_frame: export("on_frame")?.typed(&store)?,
            on_suspend: export("on_suspend")?.typed(&store)?,
            on_resume: export("on_resume")?.typed(&store)?,
            store,
            memory,
            fuel_per_turn,
            fuel_burned: 0,
            turns: 0,
        })
    }

    /// Run one turn.
    pub fn run(&mut self, turn: Turn) -> Result<Outcome> {
        self.store.set_fuel(self.fuel_per_turn)?;
        let called = match turn {
            Turn::Start => self.on_start.call(&mut self.store, ()),
            Turn::Suspend => self.on_suspend.call(&mut self.store, ()),
            Turn::Resume => self.on_resume.call(&mut self.store, ()),
            Turn::Frame(bytes) => {
                let len = u32::try_from(bytes.len()).context("frame longer than u32")?;
                let ptr = self.alloc.call(&mut self.store, len)?;
                self.memory
                    .write(&mut self.store, ptr as usize, &bytes)
                    .context("copy frame into guest memory")?;
                let called = self.on_frame.call(&mut self.store, (ptr, len));
                let freed = self.free.call(&mut self.store, (ptr, len));
                called.and(freed)
            }
        };
        let remaining = self.store.get_fuel()?;
        let burned = self.fuel_per_turn - remaining;
        self.fuel_burned += burned;
        self.turns += 1;
        called.map_err(|error| anyhow!("guest turn failed after {burned} fuel: {error}"))?;
        Ok(Outcome {
            frames: core::mem::take(&mut self.store.data_mut().frames),
            fuel_burned: burned,
        })
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
) -> Result<Vec<u8>, wasmi::Error> {
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
