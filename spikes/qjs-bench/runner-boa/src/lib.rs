//! The same worker scripts on Boa, the JavaScript engine written in Rust:
//! the guest's prelude and host bindings over a Boa context instead of
//! QuickJS, with the same modes and RESULT JSON as the other runners.
//!
//! Boa has no heap limit and no interrupt, so nothing here caps a worker's
//! memory or stops a runaway script.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

use boa_engine::{
    js_string,
    object::{builtins::JsUint8Array, ObjectInitializer},
    property::Attribute,
    Context, JsArgs, JsNativeError, JsResult, JsValue, NativeFunction, Source,
};

const ENGINE: &str = "boa";
const PRELUDE: &str = include_str!("../../guest/js/prelude.js");

#[derive(Default)]
struct HostState {
    timers: BTreeMap<u32, Instant>,
    outbound: Vec<Vec<u8>>,
}

/// One Boa context with the host bindings and prelude installed.
struct Guest {
    context: Context,
    state: Rc<RefCell<HostState>>,
}

fn bytes_of(value: &JsValue, context: &mut Context) -> JsResult<Vec<u8>> {
    let object = value
        .as_object()
        .ok_or_else(|| JsNativeError::typ().with_message("expected a Uint8Array"))?;
    JsUint8Array::from_object(object.clone())?.to_vec(context)
}

/// A host function over `state`.
fn host_function(
    state: &Rc<RefCell<HostState>>,
    body: fn(&RefCell<HostState>, &[JsValue], &mut Context) -> JsResult<JsValue>,
) -> NativeFunction {
    let state = state.clone();
    // SAFETY: the closure captures only an `Rc` of plain host data, no
    // garbage-collected value, which is what `from_closure` requires.
    unsafe { NativeFunction::from_closure(move |_, args, context| body(&state, args, context)) }
}

impl Guest {
    fn new(source: &[u8]) -> Result<(Self, Duration), String> {
        let started = Instant::now();
        let mut context = Context::default();
        let state = Rc::new(RefCell::new(HostState::default()));
        let clock = Instant::now();
        let host = ObjectInitializer::new(&mut context)
            .function(
                host_function(&state, |state, args, context| {
                    let frame = bytes_of(args.get_or_undefined(0), context)?;
                    state.borrow_mut().outbound.push(frame);
                    Ok(JsValue::undefined())
                }),
                js_string!("frameOut"),
                1,
            )
            .function(
                // SAFETY: captures only an `Instant`, no garbage-collected value.
                unsafe {
                    NativeFunction::from_closure(move |_, _, _| {
                        Ok(JsValue::from(clock.elapsed().as_secs_f64() * 1000.0))
                    })
                },
                js_string!("now"),
                0,
            )
            .function(
                NativeFunction::from_fn_ptr(|_, args, context| {
                    let len = args.get_or_undefined(0).to_u32(context)? as usize;
                    let mut bytes = vec![0; len];
                    getrandom::fill(&mut bytes).map_err(|error| JsNativeError::error().with_message(error.to_string()))?;
                    Ok(JsUint8Array::from_iter(bytes, context)?.into())
                }),
                js_string!("randomBytes"),
                1,
            )
            .function(
                host_function(&state, |state, args, context| {
                    let id = args.get_or_undefined(0).to_u32(context)?;
                    let delay_ms = args.get_or_undefined(1).to_number(context)?;
                    let due = Instant::now() + Duration::from_secs_f64(delay_ms.max(0.0) / 1000.0);
                    state.borrow_mut().timers.insert(id, due);
                    Ok(JsValue::undefined())
                }),
                js_string!("timerSet"),
                2,
            )
            .function(
                host_function(&state, |state, args, context| {
                    let id = args.get_or_undefined(0).to_u32(context)?;
                    state.borrow_mut().timers.remove(&id);
                    Ok(JsValue::undefined())
                }),
                js_string!("timerClear"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(|_, args, context| {
                    let level = args.get_or_undefined(0).to_u32(context)?;
                    let message = args.get_or_undefined(1).to_string(context)?.to_std_string_escaped();
                    bench_common::log_line(level, &message);
                    Ok(JsValue::undefined())
                }),
                js_string!("log"),
                2,
            )
            .function(
                NativeFunction::from_fn_ptr(|_, args, context| {
                    let text = args.get_or_undefined(0).to_string(context)?.to_std_string_escaped();
                    Ok(JsUint8Array::from_iter(text.into_bytes(), context)?.into())
                }),
                js_string!("utf8Encode"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(|_, args, context| {
                    let bytes = bytes_of(args.get_or_undefined(0), context)?;
                    Ok(js_string!(String::from_utf8_lossy(&bytes).as_ref()).into())
                }),
                js_string!("utf8Decode"),
                1,
            )
            .build();
        let installed = context
            .register_global_property(js_string!("__host"), host, Attribute::all())
            .and_then(|()| context.eval(Source::from_bytes(PRELUDE)))
            .and_then(|_| context.eval(Source::from_bytes(source)));
        if let Err(error) = installed {
            return Err(format!("worker script threw: {error}"));
        }
        let mut guest = Guest { context, state };
        guest.run_jobs()?;
        Ok((guest, started.elapsed()))
    }

    fn call_global(&mut self, name: &str, argument: JsValue) -> Result<(), String> {
        let global = self.context.global_object();
        let function = global.get(js_string!(name), &mut self.context).map_err(|error| error.to_string())?;
        let callable = function.as_callable().ok_or_else(|| format!("{name} is not a function"))?;
        callable.call(&JsValue::undefined(), &[argument], &mut self.context).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn deliver(&mut self, frame: &[u8]) -> Result<(), String> {
        let bytes = JsUint8Array::from_iter(frame.iter().copied(), &mut self.context).map_err(|error| error.to_string())?;
        self.call_global("__truapi_deliver", bytes.into())
    }

    fn fire_timer(&mut self, id: u32) -> Result<(), String> {
        self.call_global("__truapi_fire_timer", JsValue::from(id))
    }

    fn run_jobs(&mut self) -> Result<(), String> {
        self.context.run_jobs().map_err(|error| error.to_string())
    }

    fn take_outbound(&self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.state.borrow_mut().outbound)
    }

    /// Run jobs and echo frames back until nothing is left to do right now.
    fn drain(&mut self) -> Result<(), String> {
        loop {
            self.run_jobs()?;
            let outbound = self.take_outbound();
            if outbound.is_empty() {
                return Ok(());
            }
            for frame in outbound {
                self.deliver(&frame)?;
            }
        }
    }

    fn next_timer(&self) -> Option<(u32, Instant)> {
        self.state.borrow().timers.iter().min_by_key(|(_, due)| **due).map(|(id, due)| (*id, *due))
    }
}

/// Run `source` to completion with frames echoed back: evaluate, drain
/// jobs, fire timers on wall-clock time. Returns the time to idle.
pub fn run_to_completion(source: &[u8]) -> Result<Duration, String> {
    let (mut guest, init_in) = Guest::new(source)?;
    guest.drain()?;
    while let Some((id, due)) = guest.next_timer() {
        let now = Instant::now();
        if due > now {
            std::thread::sleep(due - now);
        }
        guest.state.borrow_mut().timers.remove(&id);
        guest.fire_timer(id)?;
        guest.drain()?;
    }
    Ok(init_in)
}

/// Boot `count` contexts on the worker bundle, keeping each alive, and
/// report cold start and the memory each one adds to the process.
pub fn instances(source: &[u8], count: usize) -> Result<String, String> {
    let before = bench_common::footprint_bytes();
    let mut guests = Vec::with_capacity(count);
    let mut boot_ms = Vec::with_capacity(count);
    let mut footprints = Vec::with_capacity(count);
    for _ in 0..count {
        let (guest, booted_in) = Guest::new(source)?;
        boot_ms.push(booted_in.as_secs_f64() * 1000.0);
        footprints.push(bench_common::footprint_bytes());
        guests.push(guest);
    }
    let first_mib = bench_common::mib(footprints[0] - before);
    let rest_mib = bench_common::mib(footprints[count - 1] - footprints[0]) / (count - 1).max(1) as f64;
    let (warm_p50, _) = bench_common::percentiles(&mut boot_ms[1..].to_vec());
    Ok(format!(
        "{{\"engine\":\"{ENGINE}\",\"firstBootMs\":{:.1},\"bootP50Ms\":{warm_p50:.1},\"firstInstanceMiB\":{first_mib:.2},\"perInstanceMiB\":{rest_mib:.2},\"instances\":{count}}}",
        boot_ms[0],
    ))
}

/// Replay the corpus against a fresh context, checking every outbound frame
/// byte for byte and timing each render request until its tree comes back.
pub fn replay(source: &[u8], corpus: &str) -> Result<String, String> {
    let (mut guest, booted_in) = Guest::new(source)?;
    guest.take_outbound();
    let steps = bench_common::load_corpus(corpus);
    let mut render_ms = Vec::new();
    let mut mismatches = 0;
    for step in &steps {
        let started = Instant::now();
        guest.deliver(&step.inbound)?;
        guest.run_jobs()?;
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        if guest.take_outbound() != step.outbound {
            mismatches += 1;
        }
        if bench_common::is_render_start(&step.inbound) {
            render_ms.push(elapsed);
        }
    }
    let (p50, p95) = bench_common::percentiles(&mut render_ms);
    Ok(format!(
        "{{\"engine\":\"{ENGINE}\",\"bootMs\":{:.1},\"steps\":{},\"mismatches\":{mismatches},\"renderSamples\":{},\"renderP50Ms\":{p50:.3},\"renderP95Ms\":{p95:.3}}}",
        booted_in.as_secs_f64() * 1000.0,
        steps.len(),
        render_ms.len(),
    ))
}
