//! Golden snapshot test for the Rust dispatcher emitter.
//!
//! `cargo rustdoc` runs once per package, under the nightly named in the
//! repository's `nightly-toolchain` file, into a dedicated
//! `target/codegen-test-rustdoc/<package>` directory, off the shared
//! `target/doc/<package>.json` path that a concurrent `cargo doc` would
//! claim. Every test reads the same JSON, so the build is paid once per
//! package per run. That toolchain is required; if it is not installed the
//! test panics rather than silently passing (`rustup toolchain install
//! "$(cat nightly-toolchain)"`). `TRUAPI_NIGHTLY_TOOLCHAIN` overrides it.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

/// The dated nightly CI runs, unless `TRUAPI_NIGHTLY_TOOLCHAIN` names another.
fn nightly_toolchain() -> String {
    std::env::var("TRUAPI_NIGHTLY_TOOLCHAIN").unwrap_or_else(|_| {
        include_str!("../../../../nightly-toolchain")
            .trim()
            .to_string()
    })
}

fn quoted_strings_in_const_array(src: &str, const_name: &str) -> Vec<String> {
    let marker = format!("export const {const_name} = [");
    let start = src
        .find(&marker)
        .unwrap_or_else(|| panic!("missing {const_name}"));
    let rest = &src[start + marker.len()..];
    let end = rest
        .find("] as const")
        .unwrap_or_else(|| panic!("unterminated {const_name}"));
    let body = &rest[..end];
    let mut strings = Vec::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch != '"' {
            continue;
        }
        let mut value = String::new();
        let mut escaped = false;
        for ch in chars.by_ref() {
            if escaped {
                value.push(ch);
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                break;
            } else {
                value.push(ch);
            }
        }
        strings.push(value);
    }
    strings
}

/// Path to the rustdoc JSON of `truapi`'s protocol definitions alone, the
/// input codegen reads the API from, building it on first use.
fn produce_rustdoc_json(workspace_root: &Path) -> PathBuf {
    produce_rustdoc_json_for_package(workspace_root, "truapi", &["--no-default-features"])
}

/// Path to `package`'s rustdoc JSON built with `cargo_args`, building it on
/// first use and reusing that build for every later caller in this test
/// binary. Panics with a clear message if nightly is unavailable so CI cannot
/// pass vacuously.
fn produce_rustdoc_json_for_package(
    workspace_root: &Path,
    package: &str,
    cargo_args: &[&str],
) -> PathBuf {
    static BUILT: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    let built = BUILT.get_or_init(|| Mutex::new(HashMap::new()));
    // Held across the build so two tests asking for the same package queue
    // instead of racing into one target directory.
    let mut built = built
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = [package]
        .into_iter()
        .chain(cargo_args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(json) = built.get(&key) {
        return json.clone();
    }

    let target_dir = workspace_root
        .join("target/codegen-test-rustdoc")
        .join(key.replace(' ', "_"));
    let json = run_rustdoc_json(workspace_root, &target_dir, package, cargo_args);
    built.insert(key, json.clone());
    json
}

/// One `cargo rustdoc --output-format json` invocation on the pinned nightly, returning
/// the path to the JSON it wrote.
fn run_rustdoc_json(
    workspace_root: &Path,
    target_dir: &Path,
    package: &str,
    cargo_args: &[&str],
) -> PathBuf {
    let toolchain = nightly_toolchain();
    let mut command = Command::new("cargo");
    command
        .arg(format!("+{toolchain}"))
        .args(["rustdoc", "-p", package])
        .args(cargo_args)
        .arg("--target-dir")
        .arg(target_dir)
        .args(["--", "-Z", "unstable-options", "--output-format", "json"])
        .current_dir(workspace_root);
    let output = command.output().expect(
        "failed to spawn rustdoc; install the pinned nightly named in nightly-toolchain via rustup",
    );
    assert!(
        output.status.success(),
        "`cargo +{toolchain} rustdoc -p {package}` failed (status {}); that nightly toolchain is required.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let json_name = package.replace('-', "_");
    let json = target_dir.join(format!("doc/{json_name}.json"));
    assert!(
        json.exists(),
        "rustdoc JSON not found at {} after successful rustdoc invocation",
        json.display(),
    );
    json
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root above rust/crates/truapi-codegen")
        .to_path_buf()
}

fn workspace_tempdir(workspace: &Path) -> tempfile::TempDir {
    let parent = workspace.join("target/codegen-test-tmp");
    fs::create_dir_all(&parent).expect("create workspace codegen temp directory");
    tempfile::Builder::new()
        .prefix("golden-")
        .tempdir_in(parent)
        .expect("workspace tempdir")
}

fn rustfmt_generated(files: &[PathBuf]) {
    if files.is_empty() {
        return;
    }

    // Keep this hermetic: rustfmt otherwise walks up from the generated file
    // in the system temp directory and may inherit another checkout's config.
    let config_dir = tempfile::tempdir().expect("rustfmt config tempdir");
    let config_path = config_dir.path().join("rustfmt.toml");
    fs::write(&config_path, "edition = \"2024\"\n").expect("write rustfmt config");

    let toolchain = nightly_toolchain();
    let mut command = Command::new("rustfmt");
    command
        .arg(format!("+{toolchain}"))
        .args(["--edition", "2024", "--config-path"])
        .arg(config_path);
    for file in files {
        command.arg(file);
    }
    let output = command
        .output()
        .expect("failed to spawn rustfmt; install rustfmt for the pinned nightly via rustup");
    assert!(
        output.status.success(),
        "rustfmt failed (status {}).\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

fn prettier_generated(workspace_root: &Path, files: &[PathBuf]) {
    if files.is_empty() {
        return;
    }

    let mut command = Command::new("npm");
    command
        .args([
            "exec",
            "--yes",
            "--package=prettier@3.8.3",
            "--",
            "prettier",
            "--write",
            "--ignore-path",
            "/dev/null",
            "--config",
        ])
        .arg(workspace_root.join(".prettierrc"));
    for file in files {
        command.arg(file);
    }
    let output = command
        .current_dir(workspace_root)
        .output()
        .expect("failed to spawn `npm exec -- prettier`; run `npm ci` first");
    assert!(
        output.status.success(),
        "prettier failed (status {}).\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn golden_dispatcher_and_wire_table() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = workspace_root();

    let tempdir = workspace_tempdir(&workspace);
    let rustdoc_json = produce_rustdoc_json(&workspace);

    let out = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
        .args([
            "--input",
            rustdoc_json.to_str().unwrap(),
            "--output",
            tempdir.path().join("ts").to_str().unwrap(),
            "--rust-output",
            tempdir.path().join("rust").to_str().unwrap(),
        ])
        .output()
        .expect("run truapi-codegen");
    assert!(
        out.status.success(),
        "codegen failed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );

    // Compare the emitted files against the goldens. We assert on
    // wire_table.rs first because it's small and the diff is easy to
    // read when the wire ids drift. mod.rs is covered because the
    // runtime declares `pub mod generated;` unconditionally, so dropping
    // it stops the crate parsing at all.
    let golden_dir = manifest_dir.join("tests/golden");
    let cases = [
        ("wire_table.rs", "wire_table.rs"),
        ("dispatcher.rs", "dispatcher.rs"),
        ("mod.rs", "mod.rs"),
    ];
    for (golden_name, output_name) in cases {
        let golden = fs::read_to_string(golden_dir.join(golden_name))
            .unwrap_or_else(|e| panic!("read {golden_name}: {e}"));
        let actual = fs::read_to_string(tempdir.path().join("rust").join(output_name))
            .unwrap_or_else(|e| panic!("read generated {output_name}: {e}"));
        if golden != actual {
            // Dump actual to a sibling file for easy inspection
            // when running locally.
            let dump = manifest_dir.join(format!("tests/golden/{output_name}.actual"));
            let _ = fs::write(&dump, &actual);
            panic!(
                "golden mismatch for {output_name}; wrote actual to {}",
                dump.display()
            );
        }
    }

    // A method body should only ever reference its pre-built `T.{Method}Version`
    // codec by name (see `method_envelope_name` in `ts.rs`); `types.ts`
    // legitimately inlines `S.indexedTaggedUnion(` to define those wrapper
    // codecs themselves, which is why only `client.ts` is scanned here.
    let client_ts = fs::read_to_string(tempdir.path().join("ts").join("client.ts"))
        .unwrap_or_else(|e| panic!("read generated client.ts: {e}"));
    assert!(
        !client_ts.contains("indexedTaggedUnion"),
        "generated client.ts contains `S.indexedTaggedUnion(`; a method body should \
         reference its `T.{{Method}}Version` codec by name instead of inlining one"
    );
}

/// Idempotence guard at the integration level: running the binary twice
/// against the same input must produce identical output. This catches
/// non-determinism (HashMap iteration order, timestamps, etc.) that the
/// inline unit tests might miss because they exercise smaller APIs.
#[test]
fn binary_emission_is_idempotent() {
    let workspace = workspace_root();
    let rustdoc_json = produce_rustdoc_json(&workspace);

    // Every emitted file, not a hand-picked list: the nondeterminism this
    // guards against has landed in the TypeScript output as readily as in the
    // Rust output, and a list only covers what someone remembered to add.
    let run_once = || -> BTreeMap<PathBuf, String> {
        let tmp = workspace_tempdir(&workspace);
        let status = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
            .args([
                "--input",
                rustdoc_json.to_str().unwrap(),
                "--output",
                tmp.path().join("ts").to_str().unwrap(),
                "--rust-output",
                tmp.path().join("rust").to_str().unwrap(),
            ])
            .status()
            .expect("run truapi-codegen");
        assert!(status.success(), "codegen run failed");
        read_tree(tmp.path())
    };

    let first = run_once();
    let second = run_once();
    assert!(!first.is_empty(), "codegen emitted nothing");
    assert_eq!(
        first.keys().collect::<Vec<_>>(),
        second.keys().collect::<Vec<_>>(),
        "the two runs emitted different files"
    );
    for (path, contents) in &first {
        assert_eq!(
            contents,
            &second[path],
            "{} differs between runs",
            path.display()
        );
    }
}

/// Every file under `root`, keyed by its path relative to `root`.
fn read_tree(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).expect("read generated directory") {
            let path = entry.expect("read generated entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("path under root")
                    .to_path_buf();
                files.insert(
                    relative,
                    fs::read_to_string(&path).expect("read generated file"),
                );
            }
        }
    }
    files
}

#[test]
fn golden_host_callbacks_ts() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = workspace_root();

    let tempdir = workspace_tempdir(&workspace);
    let truapi_json = produce_rustdoc_json(&workspace);
    let runtime_json = produce_rustdoc_json_for_package(&workspace, "truapi", &[]);
    let provider_json = produce_rustdoc_json_for_package(&workspace, "truapi-provider", &[]);

    let out = Command::new(env!("CARGO_BIN_EXE_truapi-codegen"))
        .args([
            "--input",
            truapi_json.to_str().unwrap(),
            "--output",
            tempdir.path().join("ts").to_str().unwrap(),
            "--platform-input",
            runtime_json.to_str().unwrap(),
            "--platform-input",
            provider_json.to_str().unwrap(),
            "--platform-ts-output",
            tempdir.path().join("host").to_str().unwrap(),
            "--platform-wasm-adapter-output",
            tempdir.path().join("wasm").to_str().unwrap(),
            "--platform-rust-output",
            tempdir.path().join("rust-wasm").to_str().unwrap(),
        ])
        .output()
        .expect("run truapi-codegen");
    assert!(
        out.status.success(),
        "codegen failed: stdout=\n{}\nstderr=\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    prettier_generated(
        &workspace,
        &[
            tempdir.path().join("host/host-callbacks.ts"),
            tempdir.path().join("wasm/host-callbacks-adapter.ts"),
            tempdir.path().join("wasm/worker-callbacks.ts"),
        ],
    );
    rustfmt_generated(&[tempdir.path().join("rust-wasm/generated_bridge.rs")]);

    let golden_path = manifest_dir.join("tests/golden/host-callbacks.ts");
    let golden =
        fs::read_to_string(&golden_path).unwrap_or_else(|e| panic!("read host-callbacks.ts: {e}"));
    let actual = fs::read_to_string(tempdir.path().join("host/host-callbacks.ts"))
        .expect("read generated host-callbacks.ts");
    if golden != actual {
        let dump = manifest_dir.join("tests/golden/host-callbacks.ts.actual");
        let _ = fs::write(&dump, &actual);
        panic!(
            "golden mismatch for host-callbacks.ts; wrote actual to {}",
            dump.display()
        );
    }

    let adapter_golden_path = manifest_dir.join("tests/golden/host-callbacks-adapter.ts");
    let adapter_actual = fs::read_to_string(tempdir.path().join("wasm/host-callbacks-adapter.ts"))
        .expect("read generated host-callbacks-adapter.ts");
    let adapter_golden = fs::read_to_string(&adapter_golden_path).unwrap_or_default();
    if adapter_golden != adapter_actual {
        let dump = manifest_dir.join("tests/golden/host-callbacks-adapter.ts.actual");
        let _ = fs::write(&dump, &adapter_actual);
        panic!(
            "golden mismatch for host-callbacks-adapter.ts; wrote actual to {}",
            dump.display()
        );
    }

    let worker_golden_path = manifest_dir.join("tests/golden/worker-callbacks.ts");
    let worker_actual = fs::read_to_string(tempdir.path().join("wasm/worker-callbacks.ts"))
        .expect("read generated worker-callbacks.ts");
    let worker_golden = fs::read_to_string(&worker_golden_path).unwrap_or_default();
    if worker_golden != worker_actual {
        let dump = manifest_dir.join("tests/golden/worker-callbacks.ts.actual");
        let _ = fs::write(&dump, &worker_actual);
        panic!(
            "golden mismatch for worker-callbacks.ts; wrote actual to {}",
            dump.display()
        );
    }

    let wasm_bridge_golden_path = manifest_dir.join("tests/golden/wasm_bridge.rs");
    let wasm_bridge_actual =
        fs::read_to_string(tempdir.path().join("rust-wasm/generated_bridge.rs"))
            .expect("read generated wasm bridge");
    let wasm_bridge_golden = fs::read_to_string(&wasm_bridge_golden_path).unwrap_or_default();
    if wasm_bridge_golden != wasm_bridge_actual {
        let dump = manifest_dir.join("tests/golden/wasm_bridge.rs.actual");
        let _ = fs::write(&dump, &wasm_bridge_actual);
        panic!(
            "golden mismatch for wasm_bridge.rs; wrote actual to {}",
            dump.display()
        );
    }

    assert!(
        !worker_actual.contains("OPTIONAL_CALLBACK_NAMES"),
        "worker callback generation should not expose an optional callback manifest"
    );
    let mut generated_names = quoted_strings_in_const_array(&worker_actual, "CALLBACK_NAMES");
    generated_names.extend(quoted_strings_in_const_array(
        &worker_actual,
        "SUBSCRIPTION_NAMES",
    ));
    for name in generated_names {
        // Callbacks of an optional capability bind through the optional getter;
        // either way the bridge must name every callback the worker proxies.
        assert!(
            wasm_bridge_actual.contains(&format!("get_function(callbacks, \"{name}\")?"))
                || wasm_bridge_actual
                    .contains(&format!("get_optional_function(callbacks, \"{name}\")?")),
            "generated wasm bridge must bind worker callback `{name}`"
        );
    }
}
