//! BenchKit: the engines the iOS bench app measures, behind a C API.
//!
//! `benchkit_run(engine, mode, resources, count)` runs one measurement on a
//! fresh thread and returns one JSON line:
//! `{"engine":…,"mode":…,"variant":…,"ok":true,"result":{…}}`, or
//! `"ok":false` with an `"error"`. Free it with `benchkit_free`.
//!
//! Engines: `native` (QuickJS compiled for the device), `wasmi` (the wasm
//! guest interpreted), `wasm2c` (the wasm guest compiled ahead of time, with
//! the bounds-check or guard-page variant this library was built with) and
//! `boa` (the Rust JavaScript engine, same prelude and bindings).
//! Modes: `replay`, `instances` (`count` live workers), `codec`, `rxjs`,
//! `pingpong-64`, `pingpong-4k`, `memcap` (an 8 MiB cap, wasm only),
//! `oob-trap` (an out-of-bounds read is caught and the process carries on,
//! wasm2c only) and `guard-capacity` (how many 8 GiB reservations fit, up to
//! `count`).
//! `resources` is the folder holding the workload scripts and `corpus.bin`.

use std::ffi::{c_char, CStr, CString};

const GUEST_WASM: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/wasm32-wasip1/release/qjs_guest.wasm"));
const STACK_BYTES: usize = 64 << 20;
const MEMCAP_BYTES: usize = 8 << 20;

/// Run one measurement. Every argument must be a valid C string; `count`
/// is ignored by modes that do not take one.
///
/// # Safety
/// `engine`, `mode` and `resources` must be valid, NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn benchkit_run(
    engine: *const c_char,
    mode: *const c_char,
    resources: *const c_char,
    count: u32,
) -> *mut c_char {
    let engine = CStr::from_ptr(engine).to_string_lossy().into_owned();
    let mode = CStr::from_ptr(mode).to_string_lossy().into_owned();
    let resources = CStr::from_ptr(resources).to_string_lossy().into_owned();
    let line = run_on_thread(engine, mode, resources, count as usize);
    CString::new(line).expect("no NUL in a result line").into_raw()
}

/// Free a line returned by [`benchkit_run`].
///
/// # Safety
/// `line` must come from `benchkit_run` and be freed once.
#[no_mangle]
pub unsafe extern "C" fn benchkit_free(line: *mut c_char) {
    if !line.is_null() {
        drop(CString::from_raw(line));
    }
}

/// The wasm2c variant this library was built with, as a static C string.
#[no_mangle]
pub extern "C" fn benchkit_variant() -> *const c_char {
    static VARIANT: std::sync::OnceLock<CString> = std::sync::OnceLock::new();
    VARIANT.get_or_init(|| CString::new(qjs_runner_wasm2c::VARIANT).unwrap()).as_ptr()
}

fn run_on_thread(engine: String, mode: String, resources: String, count: usize) -> String {
    let (engine_name, mode_name) = (engine.clone(), mode.clone());
    let outcome = std::thread::Builder::new()
        .name(format!("bench-{engine}"))
        .stack_size(STACK_BYTES)
        .spawn(move || {
            bench_common::take_logs();
            let outcome = measure(&engine, &mode, &resources, count);
            qjs_runner_wasm2c::release_thread();
            outcome
        })
        .map_err(|error| error.to_string())
        .and_then(|thread| thread.join().map_err(|_| "the measurement panicked".to_string()))
        .and_then(|outcome| outcome);
    let head = format!(
        "\"engine\":\"{engine_name}\",\"mode\":\"{mode_name}\",\"variant\":\"{}\"",
        qjs_runner_wasm2c::VARIANT
    );
    match outcome {
        Ok(result) => format!("{{{head},\"ok\":true,\"result\":{result}}}"),
        Err(error) => format!("{{{head},\"ok\":false,\"error\":\"{}\"}}", escape(&error)),
    }
}

fn measure(engine: &str, mode: &str, resources: &str, count: usize) -> Result<String, String> {
    let read = |name: &str| std::fs::read(format!("{resources}/{name}")).map_err(|error| format!("{name}: {error}"));
    let corpus = format!("{resources}/corpus.bin");
    let unlimited_wasmi = qjs_runner::Limits { fuel_per_turn: 0, ..Default::default() };
    match (engine, mode) {
        ("native", "replay") => qjs_guest::probe::replay(&read("w-worker.js")?, &corpus),
        ("wasmi", "replay") => qjs_runner::replay(GUEST_WASM, &read("w-worker.js")?, &corpus, &Default::default())
            .map_err(|error| error.to_string()),
        ("wasm2c", "replay") => qjs_runner_wasm2c::replay(&read("w-worker.js")?, &corpus, &Default::default()),
        ("boa", "replay") => qjs_runner_boa::replay(&read("w-worker.js")?, &corpus),

        ("native", "instances") => qjs_guest::probe::instances(&read("w-worker.js")?, count),
        ("wasmi", "instances") => qjs_runner::instances(GUEST_WASM, &read("w-worker.js")?, count, &Default::default())
            .map_err(|error| error.to_string()),
        ("wasm2c", "instances") => qjs_runner_wasm2c::instances(&read("w-worker.js")?, count, &Default::default()),
        ("boa", "instances") => qjs_runner_boa::instances(&read("w-worker.js")?, count),

        ("wasm2c", "guard-capacity") => Ok(qjs_runner_wasm2c::guard_capacity(count)),
        ("wasm2c", "oob-trap") => qjs_runner_wasm2c::oob_trap(&read("w-worker.js")?, &Default::default()),

        (_, "memcap") => {
            let source = read("t1-memory.js")?;
            match engine {
                "wasmi" => {
                    let limits = qjs_runner::Limits { memory_bytes: MEMCAP_BYTES, ..unlimited_wasmi };
                    qjs_runner::run_to_completion(GUEST_WASM, &source, &limits).map_err(|error| error.to_string())?;
                },
                "wasm2c" => {
                    let limits = qjs_runner_wasm2c::Limits { memory_bytes: MEMCAP_BYTES };
                    qjs_runner_wasm2c::run_to_completion(&source, &limits)?;
                },
                _ => return Err("memcap is a wasm check".into()),
            }
            let logs = bench_common::take_logs();
            let caught = logs.iter().find(|line| line.contains("caught:")).ok_or("the script caught no error")?;
            Ok(format!("{{\"capMiB\":8,\"caught\":\"{}\"}}", escape(caught)))
        },

        (_, "codec" | "rxjs" | "pingpong-64" | "pingpong-4k") => {
            let source = read(&format!("w-{mode}.js"))?;
            match engine {
                "native" => qjs_guest::probe::run_to_completion(&source).map(drop)?,
                "wasmi" => qjs_runner::run_to_completion(GUEST_WASM, &source, &unlimited_wasmi)
                    .map(drop)
                    .map_err(|error| error.to_string())?,
                "wasm2c" => qjs_runner_wasm2c::run_to_completion(&source, &Default::default()).map(drop)?,
                "boa" => qjs_runner_boa::run_to_completion(&source).map(drop)?,
                other => return Err(format!("unknown engine {other}")),
            }
            bench_common::bench_result(&bench_common::take_logs()).ok_or_else(|| "no BENCH_RESULT line".into())
        },

        _ => Err(format!("unknown engine and mode: {engine} {mode}")),
    }
}

/// The JNI face of [`benchkit_run`] and [`benchkit_variant`] for the Android
/// bench app's `io.parity.qjsbench.BenchKit`.
#[cfg(target_os = "android")]
mod android {
    use std::ffi::{c_char, CStr, CString};

    use jni_sys::{jclass, jint, jstring, JNIEnv};

    unsafe fn read(env: *mut JNIEnv, text: jstring) -> String {
        let chars = ((**env).v1_1.GetStringUTFChars)(env, text, std::ptr::null_mut());
        let owned = CStr::from_ptr(chars).to_string_lossy().into_owned();
        ((**env).v1_1.ReleaseStringUTFChars)(env, text, chars);
        owned
    }

    unsafe fn make(env: *mut JNIEnv, text: &str) -> jstring {
        let text = CString::new(text).expect("no NUL in a result line");
        ((**env).v1_1.NewStringUTF)(env, text.as_ptr() as *const c_char)
    }

    /// `BenchKit.run(engine, mode, resources, count)`.
    ///
    /// # Safety
    /// Called by the JVM with a valid environment and string references.
    #[no_mangle]
    pub unsafe extern "system" fn Java_io_parity_qjsbench_BenchKit_run(
        env: *mut JNIEnv,
        _class: jclass,
        engine: jstring,
        mode: jstring,
        resources: jstring,
        count: jint,
    ) -> jstring {
        let line = super::run_on_thread(
            read(env, engine),
            read(env, mode),
            read(env, resources),
            count.max(0) as usize,
        );
        make(env, &line)
    }

    /// `BenchKit.variant()`.
    ///
    /// # Safety
    /// Called by the JVM with a valid environment.
    #[no_mangle]
    pub unsafe extern "system" fn Java_io_parity_qjsbench_BenchKit_variant(env: *mut JNIEnv, _class: jclass) -> jstring {
        make(env, qjs_runner_wasm2c::VARIANT)
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}
