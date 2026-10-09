//! Runs a user host-script under `bun`, driving a host through the injected
//! `truapi` global.
//!
//! The Rust CLI owns the flow: it starts the host, then spawns `js/runner.ts`
//! (which connects the `@parity/truapi` client to the host and evaluates the
//! user script). The child's exit status becomes the host command's status, so
//! `truapi-host pairing-host --script foo.ts` *is* the test — there is no
//! separate bun orchestrator.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::process::Stdio;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::script_project;
use crate::terminal_ui::{self, SystemEvent, UiHandle};

/// Host topology serving the product script.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptHostRole {
    PairingHost,
    SigningHost,
}

impl ScriptHostRole {
    fn as_env_value(self) -> &'static str {
        match self {
            Self::PairingHost => "pairing-host",
            Self::SigningHost => "signing-host",
        }
    }
}

/// Runner bundle shipped next to the binary in a release archive. It has
/// `@parity/truapi` compiled in, so a downloaded install runs product scripts
/// without a source checkout.
const PACKAGED_RUNNER: &str = "runner.js";
const EMPTY_BUN_CONFIG: &str = if cfg!(windows) {
    "--config=NUL"
} else {
    "--config=/dev/null"
};

/// Self-contained injected-global declarations shipped with the runner.
const PACKAGED_SCRIPT_TYPES: &str = "script-types.d.ts";

/// Declaration bundle matching the selected host-script runner.
fn runner_types_path(runner: &Path) -> PathBuf {
    runner.with_file_name(PACKAGED_SCRIPT_TYPES)
}

/// Read declarations matching the selected installed or checkout runner.
pub fn script_types() -> Result<Vec<u8>> {
    let path = runner_types_path(&runner_path());
    fs::read(&path).with_context(|| {
        format!(
            "read host-script types {}; for a source build, run `make codegen` at the repository root; for a prebuilt CLI, reinstall it",
            path.display()
        )
    })
}

/// Locate the host-script runner.
pub fn runner_path() -> PathBuf {
    resolve_runner(
        std::env::var_os("TRUAPI_HOST_RUNNER"),
        std::env::current_exe().ok().as_deref(),
    )
}

/// Explicit override first, then the bundle beside the running binary, then the
/// checkout's `js/runner.ts`.
///
/// The checkout copy imports `@parity/truapi` by relative path, so it only
/// works from a built source tree; the packaged bundle is what makes an
/// installed binary self-sufficient.
fn resolve_runner(explicit: Option<OsString>, executable: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return PathBuf::from(path);
    }
    if let Some(packaged) = executable.and_then(packaged_runner) {
        return packaged;
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts")
}

fn packaged_runner(executable: &Path) -> Option<PathBuf> {
    let executable = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    let directory = executable.parent()?;
    // `current` can move during an update; the runner must stay on this binary's version.
    if let Some(versions) = directory.parent()
        && versions.file_name().is_some_and(|name| name == "versions")
    {
        return Some(
            versions
                .join(env!("CARGO_PKG_VERSION"))
                .join(PACKAGED_RUNNER),
        );
    }
    let runner = directory.join(PACKAGED_RUNNER);
    runner.is_file().then_some(runner)
}

/// Open the script in the configured terminal editor and wait for it to exit.
pub async fn edit(script: &Path) -> Result<ExitStatus> {
    let script = fs::canonicalize(script).context("resolve script path")?;
    let specification = configured_editor();
    let (program, arguments) = parse_editor(&specification)?;
    let mut command = Command::new(program);
    if let Some(directory) = script_project::directory(&script)? {
        command.current_dir(directory);
    }
    command
        .args(arguments)
        .arg(script)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command
        .status()
        .await
        .with_context(|| format!("failed to launch editor {specification:?}"))
}

fn configured_editor() -> String {
    std::env::var("VISUAL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("EDITOR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        })
}

fn parse_editor(specification: &str) -> Result<(String, Vec<String>)> {
    let mut parts = shlex::split(specification)
        .with_context(|| format!("invalid editor command {specification:?}"))?
        .into_iter();
    let program = parts
        .next()
        .with_context(|| format!("editor command is empty: {specification:?}"))?;
    Ok((program, parts.collect()))
}

/// Run `script` against the host serving frames at `frame_url`, as product
/// `product_id`. Inherits stdio so the script's output and any CLI confirmation
/// prompts share the terminal. Returns the child's exit status.
pub async fn run(
    frame_url: &str,
    product_id: &str,
    script: &Path,
    host_role: ScriptHostRole,
) -> Result<ExitStatus> {
    script_project::prepare(script, None).await?;
    eprintln!("Running {} for {product_id}", script.display());
    let mut command = command(frame_url, product_id, script, host_role)?;
    terminal_ui::output_event(SystemEvent::ScriptStarted);
    command
        .status()
        .await
        .context("failed to spawn `bun` for the host script (is bun installed?)")
}

/// Run a product script with stdout and stderr streamed into the terminal UI.
pub async fn run_captured(
    frame_url: &str,
    product_id: &str,
    script: &Path,
    ui: UiHandle,
    host_role: ScriptHostRole,
) -> Result<ExitStatus> {
    script_project::prepare(script, Some(ui.clone())).await?;
    ui.script_stdout(format!("Running {} for {product_id}", script.display()));
    let command = command(frame_url, product_id, script, host_role)?;
    terminal_ui::output_event(SystemEvent::ScriptStarted);
    capture(command, ui).await
}

/// Stream a child process into the terminal UI and stop it on cancellation.
pub async fn capture(mut command: Command, ui: UiHandle) -> Result<ExitStatus> {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("failed to spawn bun (is bun installed?)")?;
    let stdout = child.stdout.take().context("capture script stdout")?;
    let stderr = child.stderr.take().context("capture script stderr")?;
    let stdout_ui = ui.clone();
    let stdout_task = async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            stdout_ui.script_stdout(line);
        }
        Ok::<(), std::io::Error>(())
    };
    let stderr_task = async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Some(line) = lines.next_line().await? {
            ui.script_stderr(line);
        }
        Ok::<(), std::io::Error>(())
    };
    let (status, stdout, stderr) = tokio::join!(child.wait(), stdout_task, stderr_task);
    stdout.context("read script stdout")?;
    stderr.context("read script stderr")?;
    status.context("wait for host script")
}

fn command(
    frame_url: &str,
    product_id: &str,
    script: &Path,
    host_role: ScriptHostRole,
) -> Result<Command> {
    let runner = runner_path();
    if !runner.exists() {
        anyhow::bail!(
            "host-script runner not found at {}; set TRUAPI_HOST_RUNNER",
            runner.display()
        );
    }
    let runner = runner
        .canonicalize()
        .with_context(|| format!("resolve host-script runner {}", runner.display()))?;
    let script = script
        .canonicalize()
        .with_context(|| format!("script not found: {}", script.display()))?;

    let directory = script_project::directory(&script)?.unwrap_or(std::env::current_dir()?);
    let mut command = bun_command(&runner, &directory)?;
    command
        .env("TRUAPI_FRAME_URL", frame_url)
        .env("TRUAPI_PRODUCT_ID", product_id)
        .env("TRUAPI_SCRIPT", &script)
        .env("TRUAPI_CLI_HOST_ROLE", host_role.as_env_value());
    Ok(command)
}

fn bun_command(entrypoint: &Path, caller_directory: &Path) -> Result<Command> {
    let entrypoint = entrypoint
        .canonicalize()
        .with_context(|| format!("trusted Bun entrypoint not found: {}", entrypoint.display()))?;
    let directory = entrypoint
        .parent()
        .context("Bun entrypoint has no parent directory")?;
    let mut command = Command::new("bun");
    command
        .args([
            EMPTY_BUN_CONFIG,
            "--no-env-file",
            "--no-macros",
            "--install=fallback",
        ])
        .arg("run")
        .arg(&entrypoint)
        .current_dir(directory)
        .env("TRUAPI_SCRIPT_CWD", caller_directory);
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires Bun; run with --include-ignored"]
    async fn caller_configuration_cannot_execute_before_the_sandbox() -> Result<()> {
        const TEST_FIXTURE: &str = "TRUAPI_BUN_LAUNCHER_TEST_FIXTURE";
        if let Some(fixture) = std::env::var_os(TEST_FIXTURE) {
            let fixture = PathBuf::from(fixture);
            let product = std::env::current_dir()?;
            let output = bun_command(&fixture.join("trusted/entry.ts"), &product)?
                .env_remove("TRUAPI_UNTRUSTED_DOTENV")
                .output()
                .await?;
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8(output.stderr)?, "");
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&output.stdout)?,
                serde_json::json!({
                    "preloaded": false,
                    "environment": null,
                    "dependency": "trusted dependency",
                    "caller": product,
                })
            );
            return Ok(());
        }

        let fixture = tempfile::tempdir()?;
        let product = fixture.path().join("product");
        let trusted = fixture.path().join("trusted");
        fs::create_dir_all(&product)?;
        fs::create_dir_all(trusted.join("node_modules/trusted-dependency"))?;
        fs::write(
            product.join("bunfig.toml"),
            "preload = [\"./preload.ts\"]\n",
        )?;
        fs::write(
            product.join("preload.ts"),
            "globalThis.preloadExecuted = true; console.log('untrusted preload executed');\n",
        )?;
        fs::write(product.join(".env"), "TRUAPI_UNTRUSTED_DOTENV=loaded\n")?;
        fs::write(product.join("tsconfig.json"), "not valid JSON")?;
        fs::write(
            trusted.join("node_modules/trusted-dependency/package.json"),
            r#"{"name":"trusted-dependency","main":"index.js"}"#,
        )?;
        fs::write(
            trusted.join("node_modules/trusted-dependency/index.js"),
            "export const value = 'trusted dependency';\n",
        )?;
        let entrypoint = trusted.join("entry.ts");
        fs::write(
            &entrypoint,
            r#"import { value } from 'trusted-dependency';
console.log(JSON.stringify({
  preloaded: globalThis.preloadExecuted === true,
  environment: process.env.TRUAPI_UNTRUSTED_DOTENV ?? null,
  dependency: value,
  caller: process.env.TRUAPI_SCRIPT_CWD,
}));
"#,
        )?;
        let output = Command::new(std::env::current_exe()?)
            .args([
                "--ignored",
                "--exact",
                "script_runner::tests::caller_configuration_cannot_execute_before_the_sandbox",
            ])
            .current_dir(&product)
            .env(TEST_FIXTURE, fixture.path())
            .output()
            .await?;
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    /// The override exists so a packaged install can be pointed at a working
    /// copy; it has to win over the bundle sitting next to the binary.
    #[test]
    fn an_explicit_runner_overrides_the_packaged_bundle() {
        let install = tempfile::tempdir().unwrap();
        let executable = install.path().join("truapi-host");
        fs::write(install.path().join(PACKAGED_RUNNER), "packaged").unwrap();

        assert_eq!(
            resolve_runner(
                Some(OsString::from("/somewhere/custom.ts")),
                Some(&executable)
            ),
            Path::new("/somewhere/custom.ts")
        );
    }

    /// What makes a downloaded binary able to run product scripts at all.
    #[test]
    fn a_bundle_beside_the_binary_is_preferred_over_the_checkout() {
        let install = tempfile::tempdir().unwrap();
        let executable = install.path().join("truapi-host");
        let bundle = install.path().join(PACKAGED_RUNNER);
        fs::write(&executable, "binary").unwrap();
        fs::write(&bundle, "packaged").unwrap();

        assert_eq!(
            resolve_runner(None, Some(&executable)),
            bundle.canonicalize().unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_packaged_bundle_tracks_the_running_version_through_installer_symlinks() -> Result<()> {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir()?;
        let root = home.path().join("share/truapi-host");
        let version = root.join("versions").join(env!("CARGO_PKG_VERSION"));
        let bin = home.path().join("bin");
        fs::create_dir_all(&version)?;
        fs::create_dir_all(&bin)?;

        let installed_executable = version.join("truapi-host");
        let installed_bundle = version.join(PACKAGED_RUNNER);
        fs::write(&installed_executable, "binary")?;
        fs::write(&installed_bundle, "packaged")?;
        let current = root.join("current");
        symlink(
            Path::new("versions").join(env!("CARGO_PKG_VERSION")),
            &current,
        )?;

        let entrypoint = bin.join("truapi-host");
        symlink(root.join("current/truapi-host"), &entrypoint)?;

        let expected = installed_bundle.canonicalize()?;
        assert_eq!(resolve_runner(None, Some(&entrypoint)), expected);

        let next_version = root.join("versions/next");
        fs::create_dir_all(&next_version)?;
        fs::write(next_version.join("truapi-host"), "next binary")?;
        fs::write(next_version.join(PACKAGED_RUNNER), "next runner")?;
        fs::remove_file(&current)?;
        symlink("versions/next", current)?;

        assert_eq!(
            resolve_runner(None, Some(&entrypoint)),
            expected,
            "a running binary keeps using its matching runner after current moves"
        );
        Ok(())
    }

    #[test]
    fn a_source_build_falls_back_to_the_checkout_runner() {
        let install = tempfile::tempdir().unwrap();
        let executable = install.path().join("truapi-host");

        assert_eq!(
            resolve_runner(None, Some(&executable)),
            Path::new(env!("CARGO_MANIFEST_DIR")).join("js/runner.ts")
        );
    }

    #[test]
    fn an_incomplete_managed_install_does_not_fall_back_to_source_code() -> Result<()> {
        let install = tempfile::tempdir()?;
        let version = install
            .path()
            .join("versions")
            .join(env!("CARGO_PKG_VERSION"));
        fs::create_dir_all(&version)?;
        let executable = version.join("truapi-host");
        fs::write(&executable, "binary")?;
        assert_eq!(
            resolve_runner(None, Some(&executable)),
            version.canonicalize()?.join(PACKAGED_RUNNER)
        );
        Ok(())
    }

    #[test]
    fn host_scripts_are_run_by_bun() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let script = temporary.path().join("script.ts");
        fs::write(&script, "console.log('hello');\n")?;

        let runner = runner_path().canonicalize()?;
        let command = command(
            "ws://127.0.0.1:1234",
            "example.dot",
            &script,
            ScriptHostRole::SigningHost,
        )?;
        let command = command.as_std();
        assert_eq!(command.get_program(), std::ffi::OsStr::new("bun"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                std::ffi::OsStr::new(EMPTY_BUN_CONFIG),
                std::ffi::OsStr::new("--no-env-file"),
                std::ffi::OsStr::new("--no-macros"),
                std::ffi::OsStr::new("--install=fallback"),
                std::ffi::OsStr::new("run"),
                runner.as_os_str(),
            ]
        );
        assert_eq!(command.get_current_dir(), runner.parent());
        assert_eq!(
            command
                .get_envs()
                .find_map(|(key, value)| { (key == "TRUAPI_CLI_HOST_ROLE").then_some(value) }),
            Some(Some(std::ffi::OsStr::new("signing-host")))
        );
        Ok(())
    }

    #[test]
    fn relative_runner_overrides_work_in_managed_projects() -> Result<()> {
        if let Some(script) = std::env::var_os("TRUAPI_TEST_SCRIPT") {
            let command = command(
                "ws://127.0.0.1:1234",
                "example.dot",
                Path::new(&script),
                ScriptHostRole::PairingHost,
            )?;
            let runner = command.as_std().get_args().last().unwrap();
            assert_eq!(Path::new(runner), fs::canonicalize("runner.js")?);
            return Ok(());
        }
        let temporary = tempfile::tempdir()?;
        fs::write(temporary.path().join("runner.js"), "runner")?;
        let project = temporary.path().join("project");
        fs::create_dir(&project)?;
        fs::write(
            project.join("package.json"),
            r#"{"truapiHost":{"script":"script.ts"}}"#,
        )?;
        let script = project.join("script.ts");
        fs::write(&script, "script")?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "script_runner::tests::relative_runner_overrides_work_in_managed_projects",
            ])
            .current_dir(temporary.path())
            .env("TRUAPI_HOST_RUNNER", "runner.js")
            .env("TRUAPI_TEST_SCRIPT", script)
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        Ok(())
    }

    #[test]
    fn editor_command_accepts_quoted_arguments_without_a_shell() -> Result<()> {
        let (program, arguments) = parse_editor("code --wait \"profile one\"")?;

        assert_eq!(program, "code");
        assert_eq!(arguments, ["--wait", "profile one"]);
        Ok(())
    }
}
