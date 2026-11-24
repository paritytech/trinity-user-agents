//! Translates the QuickJS guest to C with wasm2c and compiles it, the wasm2c
//! runtime and the import glue into this runner.
//!
//! - `QJS_GUEST_WASM`: the guest module (default: the workspace's
//!   `wasm32-wasip1` release build).
//! - `QJS_W2C_MEMCHECK`: `guard` (guard pages and a signal handler, the
//!   default) or `bounds` (an explicit check on every load and store).
//! - `QJS_W2C_OPT`: the C optimisation level (default `2`).
//! - `WABT_PREFIX`: the WABT install (default: `brew --prefix wabt`).

use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    for variable in ["QJS_GUEST_WASM", "QJS_W2C_MEMCHECK", "QJS_W2C_OPT", "WABT_PREFIX"] {
        println!("cargo:rerun-if-env-changed={variable}");
    }
    println!("cargo:rerun-if-changed=c/glue.c");

    let guest = env::var("QJS_GUEST_WASM")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../target/wasm32-wasip1/release/qjs_guest.wasm"));
    println!("cargo:rerun-if-changed={}", guest.display());
    let memcheck = env::var("QJS_W2C_MEMCHECK").unwrap_or_else(|_| "guard".into());
    let opt = env::var("QJS_W2C_OPT").unwrap_or_else(|_| "2".into());
    let wabt = env::var("WABT_PREFIX").map(PathBuf::from).unwrap_or_else(|_| brew_prefix("wabt"));

    let status = Command::new(wabt.join("bin/wasm2c"))
        .arg(&guest)
        .args(["-n", "qjs_guest", "-o"])
        .arg(out.join("qjs_guest.c"))
        .status()
        .expect("run wasm2c");
    assert!(status.success(), "wasm2c failed on {}", guest.display());

    let runtime = wabt.join("share/wabt/wasm2c");
    let mut build = cc::Build::new();
    build
        .file(out.join("qjs_guest.c"))
        .file(runtime.join("wasm-rt-impl.c"))
        .file(runtime.join("wasm-rt-mem-impl.c"))
        .file(runtime.join("wasm-rt-exceptions-impl.c"))
        .file(manifest.join("c/glue.c"))
        .include(&out)
        .include(wabt.join("include"))
        .include(&runtime)
        .opt_level_str(&opt)
        .warnings(false)
        .flag("-w");
    match memcheck.as_str() {
        "guard" => {
            build.define("WASM_RT_MEMCHECK_GUARD_PAGES", "1");
        },
        "bounds" => {
            build.define("WASM_RT_MEMCHECK_BOUNDS_CHECK", "1");
            // Without the signal handler, wasm2c counts call depth instead
            // of catching native stack overflow. QuickJS recurses deeply.
            build.define("WASM_RT_MAX_CALL_STACK_DEPTH", "20000");
        },
        other => panic!("QJS_W2C_MEMCHECK must be guard or bounds, not {other}"),
    }
    build.compile("qjs_w2c");
    println!("cargo:rustc-env=QJS_W2C_VARIANT=wasm2c-{memcheck}-O{opt}");
}

fn brew_prefix(formula: &str) -> PathBuf {
    let output = Command::new("brew").args(["--prefix", formula]).output().expect("run brew --prefix");
    assert!(output.status.success(), "brew --prefix {formula} failed; set WABT_PREFIX");
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}
