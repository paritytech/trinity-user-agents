//! Embedded wasmi sandbox for Worker products, exposed to Kotlin over JNI.
//!
//! One sandbox per handle. Kotlin drives it turn by turn: `create` compiles
//! and instantiates the module under a fuel budget and a memory cap, `turn`
//! runs one entry point and returns the frames the guest emitted, `takeLogs`
//! drains what the guest logged, `destroy` frees the handle. A faulted
//! sandbox stays poisoned until destroyed; see [`sandbox::Sandbox::run`].
//!
//! Handles live in a process-wide registry guarded by one mutex. Turns on
//! different handles serialize through it, which matches how the Kotlin side
//! drives them: one VM thread per worker and no re-entrancy.

#![allow(non_snake_case)]

mod sandbox;

pub use sandbox::{Error, Outcome, Sandbox, Turn};

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Turn discriminants shared with Kotlin.
pub mod turn_kind {
    /// The connection opened.
    pub const START: i32 = 0;
    /// One frame from the host.
    pub const FRAME: i32 = 1;
    /// The worker is being paused.
    pub const SUSPEND: i32 = 2;
    /// The worker is being resumed.
    pub const RESUME: i32 = 3;
}

struct Registry {
    next: i64,
    sandboxes: HashMap<i64, Sandbox>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(Registry {
            next: 1,
            sandboxes: HashMap::new(),
        })
    })
}

/// Instantiate `module` and register it. Returns the handle.
pub fn create(module: &[u8], fuel_per_turn: u64, memory_bytes: usize) -> sandbox::Result<i64> {
    let sandbox = Sandbox::new(module, fuel_per_turn, memory_bytes)?;
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let handle = registry.next;
    registry.next += 1;
    registry.sandboxes.insert(handle, sandbox);
    Ok(handle)
}

/// Run one turn on `handle`.
pub fn turn(handle: i64, turn: Turn) -> sandbox::Result<Outcome> {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let sandbox = registry
        .sandboxes
        .get_mut(&handle)
        .ok_or_else(|| Error::Module(format!("no sandbox for handle {handle}")))?;
    sandbox.run(turn)
}

/// Lines the guest logged on `handle` since the last call; empty for an unknown handle.
pub fn take_logs(handle: i64) -> Vec<String> {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry
        .sandboxes
        .get_mut(&handle)
        .map(Sandbox::take_logs)
        .unwrap_or_default()
}

/// Free `handle`. Unknown handles are ignored.
pub fn destroy(handle: i64) {
    let mut registry = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.sandboxes.remove(&handle);
}

/// Decode a turn discriminant and its optional frame.
pub fn turn_from_parts(kind: i32, frame: Option<Vec<u8>>) -> Option<Turn> {
    match (kind, frame) {
        (turn_kind::START, None) => Some(Turn::Start),
        (turn_kind::FRAME, Some(bytes)) => Some(Turn::Frame(bytes)),
        (turn_kind::SUSPEND, None) => Some(Turn::Suspend),
        (turn_kind::RESUME, None) => Some(Turn::Resume),
        _ => None,
    }
}

#[cfg(target_os = "android")]
mod jni_surface {
    //! `io.paritytech.polkadotapp.wasmi_worker.WasmiWorkerNative`.

    use jni::objects::{JByteArray, JClass, JObjectArray};
    use jni::sys::{jint, jlong, jobjectArray};
    use jni::JNIEnv;

    use crate::{create, destroy, take_logs, turn, turn_from_parts, Error, Turn};

    const EXCEPTION: &str = "java/lang/IllegalStateException";

    fn throw(env: &mut JNIEnv<'_>, error: impl std::fmt::Display) {
        let _ = env.throw_new(EXCEPTION, error.to_string());
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_paritytech_polkadotapp_wasmi_1worker_WasmiWorkerNative_create(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        module: JByteArray<'_>,
        fuel_per_turn: jlong,
        memory_bytes: jlong,
    ) -> jlong {
        let module = match env.convert_byte_array(&module) {
            Ok(bytes) => bytes,
            Err(error) => {
                throw(&mut env, error);
                return 0;
            }
        };
        let fuel = u64::try_from(fuel_per_turn).unwrap_or(0);
        let memory = usize::try_from(memory_bytes).unwrap_or(0);
        match create(&module, fuel, memory) {
            Ok(handle) => handle,
            Err(error) => {
                throw(&mut env, error);
                0
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_paritytech_polkadotapp_wasmi_1worker_WasmiWorkerNative_turn(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        handle: jlong,
        kind: jint,
        frame: JByteArray<'_>,
    ) -> jobjectArray {
        let frame = if frame.is_null() {
            None
        } else {
            match env.convert_byte_array(&frame) {
                Ok(bytes) => Some(bytes),
                Err(error) => {
                    throw(&mut env, error);
                    return std::ptr::null_mut();
                }
            }
        };
        let Some(turn_value) = turn_from_parts(kind, frame) else {
            throw(&mut env, Error::Module(format!("invalid turn kind {kind}")));
            return std::ptr::null_mut();
        };
        let outcome = match turn(handle, turn_value) {
            Ok(outcome) => outcome,
            Err(error) => {
                throw(&mut env, error);
                return std::ptr::null_mut();
            }
        };
        byte_arrays(&mut env, &outcome.frames)
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_paritytech_polkadotapp_wasmi_1worker_WasmiWorkerNative_takeLogs(
        mut env: JNIEnv<'_>,
        _class: JClass<'_>,
        handle: jlong,
    ) -> jobjectArray {
        let logs = take_logs(handle);
        let string_class = match env.find_class("java/lang/String") {
            Ok(class) => class,
            Err(error) => {
                throw(&mut env, error);
                return std::ptr::null_mut();
            }
        };
        let array = match env.new_object_array(
            logs.len() as i32,
            string_class,
            jni::objects::JObject::null(),
        ) {
            Ok(array) => array,
            Err(error) => {
                throw(&mut env, error);
                return std::ptr::null_mut();
            }
        };
        for (index, line) in logs.iter().enumerate() {
            let value = match env.new_string(line) {
                Ok(value) => value,
                Err(error) => {
                    throw(&mut env, error);
                    return std::ptr::null_mut();
                }
            };
            if let Err(error) = env.set_object_array_element(&array, index as i32, value) {
                throw(&mut env, error);
                return std::ptr::null_mut();
            }
        }
        array.into_raw()
    }

    #[unsafe(no_mangle)]
    pub extern "system" fn Java_io_paritytech_polkadotapp_wasmi_1worker_WasmiWorkerNative_destroy(
        _env: JNIEnv<'_>,
        _class: JClass<'_>,
        handle: jlong,
    ) {
        destroy(handle);
    }

    fn byte_arrays(env: &mut JNIEnv<'_>, frames: &[Vec<u8>]) -> jobjectArray {
        let element_class = match env.find_class("[B") {
            Ok(class) => class,
            Err(error) => {
                throw(env, error);
                return std::ptr::null_mut();
            }
        };
        let array: JObjectArray<'_> = match env.new_object_array(
            frames.len() as i32,
            element_class,
            jni::objects::JObject::null(),
        ) {
            Ok(array) => array,
            Err(error) => {
                throw(env, error);
                return std::ptr::null_mut();
            }
        };
        for (index, frame) in frames.iter().enumerate() {
            let value = match env.byte_array_from_slice(frame) {
                Ok(value) => value,
                Err(error) => {
                    throw(env, error);
                    return std::ptr::null_mut();
                }
            };
            if let Err(error) = env.set_object_array_element(&array, index as i32, value) {
                throw(env, error);
                return std::ptr::null_mut();
            }
        }
        array.into_raw()
    }

    // Keeps the `Turn` import used on every build of this module.
    #[allow(dead_code)]
    fn _turn_type_is_used(_: Turn) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Pocket build of the counter guest from the TrUAPI repository, built
    /// with `cargo build -p wasm-worker-probe-guest --target wasm32-unknown-unknown --release --features pocket`.
    fn pocket_guest() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../../target/wasm32-unknown-unknown/release/wasm_worker_probe_guest.wasm"
        );
        std::fs::read(path).unwrap_or_else(|error| panic!("read {path}: {error}"))
    }

    fn method_of(frame: &[u8]) -> (u8, u8) {
        // [requestId: SCALE str][trait][method][type][payload]; request ids here are short.
        let len = (frame[0] >> 2) as usize;
        (frame[1 + len], frame[2 + len])
    }

    #[test]
    fn start_sends_handshake_and_action_subscription_only() {
        let handle = create(&pocket_guest(), 50_000_000, 16 * 1024 * 1024).expect("instantiates");
        let outcome = turn(handle, Turn::Start).expect("start runs");
        assert_eq!(
            outcome
                .frames
                .iter()
                .map(|frame| method_of(frame))
                .collect::<Vec<_>>(),
            vec![(1, 0), (17, 1)]
        );
        assert!(outcome.fuel_burned > 0);
        destroy(handle);
    }

    #[test]
    fn a_fuel_trap_poisons_the_sandbox() {
        let handle = create(&pocket_guest(), 1_000, 16 * 1024 * 1024).expect("instantiates");
        let first = turn(handle, Turn::Start);
        assert!(matches!(first, Err(Error::Trap { .. })), "{first:?}");
        let second = turn(handle, Turn::Resume);
        assert!(matches!(second, Err(Error::Poisoned)), "{second:?}");
        destroy(handle);
    }

    #[test]
    fn unknown_handles_and_turn_kinds_are_errors_not_panics() {
        assert!(matches!(turn(999_999, Turn::Start), Err(Error::Module(_))));
        assert!(turn_from_parts(turn_kind::FRAME, None).is_none());
        assert!(turn_from_parts(7, None).is_none());
        destroy(999_999);
    }

    #[test]
    fn suspend_and_resume_produce_no_frames_before_a_render_is_open() {
        let handle = create(&pocket_guest(), 50_000_000, 16 * 1024 * 1024).expect("instantiates");
        turn(handle, Turn::Start).expect("start runs");
        assert!(turn(handle, Turn::Suspend)
            .expect("suspend runs")
            .frames
            .is_empty());
        assert!(turn(handle, Turn::Resume)
            .expect("resume runs")
            .frames
            .is_empty());
        destroy(handle);
    }
}
