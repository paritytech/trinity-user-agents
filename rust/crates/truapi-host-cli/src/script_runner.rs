//! Runs a user host-script under `bun`, driving a host through the injected
//! `truapi` global.
//!
//! The Rust CLI owns the flow: it starts the host, then spawns `js/runner.ts`
//! (which connects the `@parity/truapi` client to the host and evaluates the
//! user script). The child's exit status becomes the host command's status, so
//! `truapi-host pairing-host --script foo.ts` *is* the test — there is no
//! separate bun orchestrator.

use std::{
	ffi::OsString,
	fs,
	path::{Path, PathBuf},
	process::{ExitStatus, Stdio},
};

use anyhow::{Context, Result};
use tokio::{
	io::{AsyncBufReadExt, BufReader},
	process::Command,
};

use truapi::platform::ProductId;

use crate::{
	script_project,
	terminal_ui::{self, SystemEvent, UiHandle},
};

/// Host topology serving the product script. Its `&'static str` form is the
/// value of `TRUAPI_CLI_HOST_ROLE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum ScriptHostRole {
	PairingHost,
	SigningHost,
}

/// Runner bundle shipped next to the binary in a release archive. It has
/// `@parity/truapi` compiled in, so a downloaded install runs product scripts
/// without a source checkout.
const PACKAGED_RUNNER: &str = "runner.js";
const EMPTY_BUN_CONFIG: &str = if cfg!(windows) { "--config=NUL" } else { "--config=/dev/null" };

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
	resolve_runner(std::env::var_os("TRUAPI_HOST_RUNNER"), std::env::current_exe().ok().as_deref())
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
	if let Some(versions) = directory.parent() &&
		versions.file_name().is_some_and(|name| name == "versions")
	{
		return Some(versions.join(env!("CARGO_PKG_VERSION")).join(PACKAGED_RUNNER));
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
		.or_else(|| std::env::var("EDITOR").ok().filter(|value| !value.trim().is_empty()))
		.unwrap_or_else(|| if cfg!(windows) { "notepad".to_string() } else { "vi".to_string() })
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
	product_id: &ProductId,
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
	product_id: &ProductId,
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

/// Run `script` as `product_id`'s worker until it exits, killing it if the
/// returned future is dropped first. Its output goes to the terminal UI when
/// there is one, and to this process's own output otherwise.
pub async fn run_worker(
	frame_url: &str,
	product_id: &ProductId,
	script: &Path,
	ui: Option<UiHandle>,
) -> Result<ExitStatus> {
	script_project::prepare(script, ui.clone()).await?;
	let mut command = command(frame_url, product_id, script, ScriptHostRole::SigningHost)?;
	// A worker runs until the host stops it, even once its module has
	// evaluated with nothing left to wait on.
	command.env("TRUAPI_SCRIPT_EXECUTION", "worker");
	// The host reads the user's commands from its own stdin.
	command.stdin(Stdio::null());
	match ui {
		Some(ui) => capture(command, ui).await,
		None => command
			.kill_on_drop(true)
			.status()
			.await
			.context("failed to spawn `bun` for the worker script (is bun installed?)"),
	}
}

/// Stream a child process into the terminal UI and stop it on cancellation.
pub async fn capture(mut command: Command, ui: UiHandle) -> Result<ExitStatus> {
	command.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
	let mut child = command.spawn().context("failed to spawn bun (is bun installed?)")?;
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
	product_id: &ProductId,
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
		.env("TRUAPI_PRODUCT_ID", product_id.as_str())
		.env("TRUAPI_SCRIPT", &script)
		.env("TRUAPI_CLI_HOST_ROLE", <&str>::from(host_role));
	Ok(command)
}

fn bun_command(entrypoint: &Path, caller_directory: &Path) -> Result<Command> {
	let entrypoint = entrypoint
		.canonicalize()
		.with_context(|| format!("trusted Bun entrypoint not found: {}", entrypoint.display()))?;
	let directory = entrypoint.parent().context("Bun entrypoint has no parent directory")?;
	let mut command = Command::new("bun");
	command
		.args([EMPTY_BUN_CONFIG, "--no-env-file", "--no-macros", "--install=fallback"])
		.arg("run")
		.arg(&entrypoint)
		.current_dir(directory)
		.env("TRUAPI_SCRIPT_CWD", caller_directory);
	Ok(command)
}

#[cfg(test)]
mod tests;
