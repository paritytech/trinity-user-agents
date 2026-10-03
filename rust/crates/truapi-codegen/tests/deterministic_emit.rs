//! Integration test for deterministic protocol code generation.
//!
//! Protocol rustdoc uses a dedicated target directory, separate from concurrent
//! runtime documentation. The nightly named in `nightly-toolchain` is required;
//! `TRUAPI_NIGHTLY_TOOLCHAIN` overrides it.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The dated nightly CI runs, unless `TRUAPI_NIGHTLY_TOOLCHAIN` names another.
fn nightly_toolchain() -> String {
    std::env::var("TRUAPI_NIGHTLY_TOOLCHAIN").unwrap_or_else(|_| {
        include_str!("../../../../nightly-toolchain")
            .trim()
            .to_string()
    })
}

/// Build the protocol definitions with the same nightly used by codegen.
fn produce_rustdoc_json(workspace_root: &Path) -> PathBuf {
    let target_dir = workspace_root
        .join("target/codegen-test-rustdoc/truapi_--no-default-features_--features_host-api");
    let toolchain = nightly_toolchain();
    let output = Command::new("cargo")
        .arg(format!("+{toolchain}"))
        .args([
            "rustdoc",
            "-p",
            "truapi",
            "--no-default-features",
            "--features",
            "host-api",
            "--target-dir",
        ])
        .arg(&target_dir)
        .args(["--", "-Z", "unstable-options", "--output-format", "json"])
        .current_dir(workspace_root)
        .output()
        .expect("failed to spawn rustdoc; install the pinned nightly named in nightly-toolchain");
    assert!(
        output.status.success(),
        "`cargo +{toolchain} rustdoc -p truapi` failed (status {}); that nightly toolchain is required.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let json = target_dir.join("doc/truapi.json");
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
