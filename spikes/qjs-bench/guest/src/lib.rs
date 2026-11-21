//! M0 benchmark guest: QuickJS (rquickjs) built for `wasm32-wasip1`.
//!
//! The host drives it through a handful of exports and gives it a handful of
//! imports in the `truapi` module; see the M0 plan for the ABI. Everything a
//! worker script can reach is installed by `install_host` and wrapped by
//! `js/prelude.js`, which then deletes the raw host object.

use std::cell::RefCell;

use rquickjs::{CatchResultExt, Context, Ctx, Function, Object, Runtime, TypedArray};

pub mod host;

const PRELUDE: &str = include_str!("../js/prelude.js");
const MEMORY_LIMIT_BYTES: usize = 24 << 20;
const MAX_STACK_BYTES: usize = 256 << 10;

struct Engine {
    runtime: Runtime,
    context: Context,
}

thread_local! {
    static ENGINE: RefCell<Option<Engine>> = const { RefCell::new(None) };
}

/// Allocate `len` bytes in guest memory for the host to write into.
#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// Free a buffer returned by [`alloc`].
///
/// # Safety
/// `pointer` and `len` must come from one earlier `alloc` call.
#[no_mangle]
pub unsafe extern "C" fn dealloc(pointer: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(pointer, 0, len));
}

/// Create the engine and evaluate the script at `pointer..pointer+len`.
/// Returns 0 on success, 1 if the script threw, 2 if the engine already exists.
///
/// # Safety
/// The range must be a UTF-8 buffer the host wrote through [`alloc`].
#[no_mangle]
pub unsafe extern "C" fn init(pointer: *const u8, len: usize) -> i32 {
    let source = std::slice::from_raw_parts(pointer, len).to_vec();
    ENGINE.with_borrow_mut(|slot| {
        if slot.is_some() {
            return 2;
        }
        let runtime = Runtime::new().expect("QuickJS runtime");
        runtime.set_memory_limit(MEMORY_LIMIT_BYTES);
        runtime.set_max_stack_size(MAX_STACK_BYTES);
        let context = Context::full(&runtime).expect("QuickJS context");
        let status = context.with(|ctx| {
            let evaluated = install_host(&ctx)
                .and_then(|()| ctx.eval::<(), _>(PRELUDE))
                .and_then(|()| ctx.eval::<(), _>(source));
            report(&ctx, evaluated)
        });
        *slot = Some(Engine { runtime, context });
        status
    })
}

/// Deliver one inbound frame to the script's frame listener.
///
/// # Safety
/// The range must be a buffer the host wrote through [`alloc`].
#[no_mangle]
pub unsafe extern "C" fn on_frame(pointer: *const u8, len: usize) {
    let frame = std::slice::from_raw_parts(pointer, len).to_vec();
    with_context(|ctx| {
        let delivered = TypedArray::<u8>::new(ctx.clone(), frame)
            .and_then(|bytes| global_function(ctx, "__truapi_deliver")?.call::<_, ()>((bytes,)));
        report(ctx, delivered);
    });
}

/// Fire the timer the host scheduled with `timer_set`.
#[no_mangle]
pub extern "C" fn on_timer(id: u32) {
    with_context(|ctx| {
        let fired = global_function(ctx, "__truapi_fire_timer").and_then(|fire| fire.call::<_, ()>((id,)));
        report(ctx, fired);
    });
}

/// Run every pending promise job. Returns how many ran, or -1 if any threw.
#[no_mangle]
pub extern "C" fn run_jobs() -> i32 {
    ENGINE.with_borrow(|slot| {
        let Some(engine) = slot else { return 0 };
        let mut ran = 0;
        let mut failed = false;
        loop {
            match engine.runtime.execute_pending_job() {
                Ok(true) => ran += 1,
                Ok(false) => break,
                Err(exception) => {
                    failed = true;
                    exception.0.with(|ctx| {
                        let caught = ctx.catch();
                        host::log(host::LEVEL_ERROR, &format!("uncaught in job: {caught:?}"));
                    });
                },
            }
        }
        if failed { -1 } else { ran }
    })
}

fn with_context(f: impl FnOnce(&Ctx<'_>)) {
    ENGINE.with_borrow(|slot| {
        if let Some(engine) = slot {
            engine.context.with(|ctx| f(&ctx));
        }
    });
}

fn global_function<'js>(ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Function<'js>> {
    ctx.globals().get(name)
}

fn report(ctx: &Ctx<'_>, result: rquickjs::Result<()>) -> i32 {
    match result.catch(ctx) {
        Ok(()) => 0,
        Err(error) => {
            host::log(host::LEVEL_ERROR, &error.to_string());
            1
        },
    }
}

/// Install the raw host bindings as `globalThis.__host`. The prelude wraps
/// them and removes `__host` before any worker code runs.
fn install_host(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let bindings = Object::new(ctx.clone())?;
    bindings.set(
        "frameOut",
        Function::new(ctx.clone(), |frame: TypedArray<'_, u8>| host::frame_out(&copy_bytes(&frame)))?,
    )?;
    bindings.set("now", Function::new(ctx.clone(), host::now_ms)?)?;
    bindings.set("randomBytes", Function::new(ctx.clone(), random_bytes)?)?;
    bindings.set("timerSet", Function::new(ctx.clone(), host::timer_set)?)?;
    bindings.set("timerClear", Function::new(ctx.clone(), host::timer_clear)?)?;
    bindings.set("log", Function::new(ctx.clone(), |level: u32, message: String| host::log(level, &message))?)?;
    bindings.set("utf8Encode", Function::new(ctx.clone(), utf8_encode)?)?;
    bindings.set(
        "utf8Decode",
        Function::new(ctx.clone(), |bytes: TypedArray<'_, u8>| String::from_utf8_lossy(&copy_bytes(&bytes)).into_owned())?,
    )?;
    ctx.globals().set("__host", bindings)
}

fn random_bytes<'js>(ctx: Ctx<'js>, len: usize) -> rquickjs::Result<TypedArray<'js, u8>> {
    TypedArray::new(ctx, host::random_bytes(len))
}

fn utf8_encode<'js>(ctx: Ctx<'js>, text: String) -> rquickjs::Result<TypedArray<'js, u8>> {
    TypedArray::new(ctx, text.into_bytes())
}

fn copy_bytes(array: &TypedArray<'_, u8>) -> Vec<u8> {
    let Some(raw) = array.as_raw() else { return Vec::new() };
    // SAFETY: copied out before any call back into the engine, as `as_raw` requires.
    unsafe { raw.as_ref().to_vec() }
}
