//! Determinism test for generated protocol and host bindings.
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

/// Path to the rustdoc JSON of `truapi`'s protocol definitions alone, the
/// input codegen reads the API from, building it on first use.
fn produce_rustdoc_json(workspace_root: &Path) -> PathBuf {
    produce_rustdoc_json_for_package(
        workspace_root,
        "truapi",
        &["--no-default-features", "--features", "host-api"],
    )
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

/// Idempotence guard at the integration level: running the binary twice
/// against the same input must produce identical output. This catches
/// non-determinism (HashMap iteration order, timestamps, etc.) that the
/// inline unit tests might miss because they exercise smaller APIs.
#[test]
fn binary_emission_is_idempotent() {
    let workspace = workspace_root();
    let rustdoc_json = produce_rustdoc_json(&workspace);
    let runtime_json = produce_rustdoc_json_for_package(&workspace, "truapi", &[]);
    let provider_json = produce_rustdoc_json_for_package(&workspace, "truapi-provider", &[]);

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
                "--platform-input",
                runtime_json.to_str().unwrap(),
                "--platform-input",
                provider_json.to_str().unwrap(),
                "--platform-ts-output",
                tmp.path().join("host").to_str().unwrap(),
                "--platform-wasm-adapter-output",
                tmp.path().join("wasm").to_str().unwrap(),
                "--platform-rust-output",
                tmp.path().join("rust-wasm").to_str().unwrap(),
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
