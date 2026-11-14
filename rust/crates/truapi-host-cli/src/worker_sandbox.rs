//! The operating-system sandbox a published worker runs in.
//!
//! A worker a provider publishes on dotNS is code fetched from the network,
//! so the host gives it no more than an app gives the worker it runs: the
//! frame socket to the host, the network the runner's gates allow, its own
//! archive to read, a scratch directory to write, and no other program to
//! execute. The host clears the worker's environment itself; this module
//! confines the process.
//!
//! The host launches the worker through its own binary,
//! `truapi-host __worker-sandbox <policy> -- <bun> <arguments>`, which
//! confines itself and then executes `bun` in its place: on macOS under
//! `sandbox-exec` with a profile generated from the policy, on Linux under a
//! Landlock ruleset it applies to itself. Neither can name the hosts a worker
//! reaches, only the ports, so the runner's gates stay what pins a worker to
//! the origins its product was granted.
//!
//! Where the platform cannot confine the worker the launch refuses to run
//! it, unless `TRUAPI_FUNDING_UNSANDBOXED=1` asks the host to run published
//! workers without the sandbox.

use std::{
	ffi::{OsStr, OsString},
	path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail, ensure};

/// The hidden first argument that has the binary confine itself and run the
/// program after `--`.
pub const SUBCOMMAND: &str = "__worker-sandbox";

/// Set to `1` to run published workers without the sandbox.
pub const UNSANDBOXED_ENV: &str = "TRUAPI_FUNDING_UNSANDBOXED";

/// The TCP ports a worker may connect to, which carry the `http:`, `https:`
/// and `wss:` traffic the runner's gates allow.
pub const TCP_PORTS: [u16; 2] = [80, 443];

/// What a confined worker may touch. Every path is absolute and canonical,
/// as the macOS sandbox matches the path a file resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
	/// The one program the worker may execute: `bun`.
	pub program: PathBuf,
	/// Trees the worker may read: the runner, its dependencies and the
	/// worker's archive.
	pub read: Vec<PathBuf>,
	/// The worker's scratch directory, the one tree it may write.
	pub scratch: PathBuf,
	/// The frame socket, the one Unix socket the worker may connect to.
	pub socket: PathBuf,
}

/// Whether the user asked for published workers to run unsandboxed.
pub fn unsandboxed() -> bool {
	std::env::var_os(UNSANDBOXED_ENV).is_some_and(|value| value == "1")
}

impl Policy {
	/// The arguments that follow [`SUBCOMMAND`] to run `program` with
	/// `arguments` under this policy.
	pub fn arguments(&self, arguments: impl IntoIterator<Item = OsString>) -> Vec<OsString> {
		let mut all: Vec<OsString> = Vec::new();
		for path in &self.read {
			all.extend([OsString::from("--read"), path.clone().into_os_string()]);
		}
		all.extend([
			OsString::from("--scratch"),
			self.scratch.clone().into_os_string(),
			OsString::from("--socket"),
			self.socket.clone().into_os_string(),
			OsString::from("--"),
			self.program.clone().into_os_string(),
		]);
		all.extend(arguments);
		all
	}

	/// Read a policy back from the arguments [`Self::arguments`] wrote,
	/// along with the program's arguments.
	pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<(Self, Vec<OsString>)> {
		let mut arguments = arguments.into_iter();
		let mut read = Vec::new();
		let mut scratch = None;
		let mut socket = None;
		loop {
			let flag = arguments.next().context("the worker sandbox names no program")?;
			if flag == "--" {
				break;
			}
			let value = arguments
				.next()
				.map(PathBuf::from)
				.with_context(|| format!("{} needs a path", flag.to_string_lossy()))?;
			ensure!(value.is_absolute(), "{} is not an absolute path", value.display());
			match flag.to_str() {
				Some("--read") => read.push(value),
				Some("--scratch") => scratch = Some(value),
				Some("--socket") => socket = Some(value),
				_ => bail!("unknown worker sandbox flag {}", flag.to_string_lossy()),
			}
		}
		let program = arguments.next().map(PathBuf::from).context("no program to run")?;
		ensure!(program.is_absolute(), "{} is not an absolute path", program.display());
		let policy = Self {
			program,
			read,
			scratch: scratch.context("no --scratch directory")?,
			socket: socket.context("no --socket")?,
		};
		Ok((policy, arguments.collect()))
	}

	/// The macOS sandbox profile enforcing this policy. It denies everything
	/// the system's base profile does not grant a process to start, then
	/// grants the policy.
	#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
	pub fn macos_profile(&self) -> String {
		let program = sbpl_string(&self.program);
		let scratch = sbpl_string(&self.scratch);
		let socket = sbpl_string(&self.socket);
		let read = self
			.read
			.iter()
			.map(|path| format!(" (subpath {})", sbpl_string(path)))
			.collect::<String>();
		let ancestors = self
			.read
			.iter()
			.chain([&self.scratch])
			.map(|path| format!(" (path-ancestors {})", sbpl_string(path)))
			.collect::<String>();
		let ports = TCP_PORTS
			.iter()
			.map(|port| format!(" (remote tcp \"*:{port}\")"))
			.collect::<String>();
		format!(
			"(version 1)\n\
			 (deny default)\n\
			 ; What every process needs to start: the system libraries, \
			 /dev/null, /dev/urandom, sysctl reads and logging.\n\
			 (import \"system.sb\")\n\
			 ; bun, the one program the worker runs.\n\
			 (allow process-exec (literal {program}))\n\
			 (allow file-read* (literal {program}))\n\
			 ; The runner, its dependencies and the worker's archive.\n\
			 (allow file-read*{read})\n\
			 ; The directories above them, to resolve paths through.\n\
			 (allow file-read-metadata{ancestors})\n\
			 ; The worker's scratch directory.\n\
			 (allow file-read* file-write* (subpath {scratch}))\n\
			 ; The frame socket to the host.\n\
			 (allow network-outbound (literal {socket}))\n\
			 ; Name resolution, and the ports the runner's network gates reach.\n\
			 (allow mach-lookup (global-name \"com.apple.dnssd.service\"))\n\
			 (allow network-outbound (literal \"/private/var/run/mDNSResponder\"))\n\
			 (allow network-outbound{ports})\n"
		)
	}
}

/// `path` as an SBPL string literal.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn sbpl_string(path: &Path) -> String {
	let mut quoted = String::from("\"");
	for character in path.to_string_lossy().chars() {
		if matches!(character, '"' | '\\') {
			quoted.push('\\');
		}
		quoted.push(character);
	}
	quoted.push('"');
	quoted
}

/// The arguments after [`SUBCOMMAND`] when this process was launched to
/// confine a worker.
pub fn requested() -> Option<Vec<OsString>> {
	let mut arguments = std::env::args_os().skip(1);
	(arguments.next().as_deref() == Some(OsStr::new(SUBCOMMAND))).then(|| arguments.collect())
}

/// Confine this process as `arguments` asks and execute the program in its
/// place. Returns only to exit, when the sandbox or the program cannot start.
pub fn enter(arguments: Vec<OsString>) -> ! {
	let error = match Policy::parse(arguments) {
		Ok((policy, arguments)) => platform::exec(&policy, arguments),
		Err(error) => error,
	};
	eprintln!("[worker sandbox] {error:#}");
	std::process::exit(126)
}

#[cfg(target_os = "macos")]
mod platform {
	use std::{ffi::OsString, os::unix::process::CommandExt, process::Command};

	use super::Policy;

	/// Run the program under `sandbox-exec` with the policy's profile.
	pub fn exec(policy: &Policy, arguments: Vec<OsString>) -> anyhow::Error {
		let error = Command::new("/usr/bin/sandbox-exec")
			.arg("-p")
			.arg(policy.macos_profile())
			.arg(&policy.program)
			.args(arguments)
			.exec();
		anyhow::anyhow!("starting sandbox-exec: {error}")
	}
}

#[cfg(target_os = "linux")]
mod platform {
	use std::{ffi::OsString, os::unix::process::CommandExt, process::Command};

	use landlock::{
		ABI, Access, AccessFs, AccessNet, BitFlags, NetPort, Ruleset, RulesetAttr,
		RulesetCreatedAttr, RulesetError, RulesetStatus, Scope, path_beneath_rules,
	};

	use super::{LandlockRules, Policy, TCP_PORTS, UNSANDBOXED_ENV};

	/// The newest Landlock ABI this ruleset asks for; an older kernel
	/// enforces what it supports.
	const ABI_REQUESTED: ABI = ABI::V9;

	/// Restrict this process with Landlock and execute the program.
	pub fn exec(policy: &Policy, arguments: Vec<OsString>) -> anyhow::Error {
		match restrict(&LandlockRules::of(policy)) {
			Ok(RulesetStatus::FullyEnforced) => {},
			Ok(RulesetStatus::PartiallyEnforced) => eprintln!(
				"[worker sandbox] this kernel enforces only part of the worker's Landlock sandbox"
			),
			Ok(RulesetStatus::NotEnforced) => {
				return anyhow::anyhow!(
					"this kernel has no Landlock, so the published worker cannot be sandboxed; \
					 set {UNSANDBOXED_ENV}=1 to run published workers without the sandbox"
				);
			},
			Err(error) => return anyhow::anyhow!("applying the Landlock sandbox: {error}"),
		}
		let error = Command::new(&policy.program).args(arguments).exec();
		anyhow::anyhow!("starting {}: {error}", policy.program.display())
	}

	fn restrict(rules: &LandlockRules) -> Result<RulesetStatus, RulesetError> {
		let read = AccessFs::ReadFile | AccessFs::ReadDir;
		// What the worker writes it cannot execute.
		let write = AccessFs::from_all(ABI_REQUESTED) & !BitFlags::from(AccessFs::Execute);
		let status = Ruleset::default()
			.handle_access(AccessFs::from_all(ABI_REQUESTED))?
			.handle_access(AccessNet::from_all(ABI_REQUESTED))?
			.scope(Scope::from_all(ABI_REQUESTED))?
			.create()?
			.add_rules(path_beneath_rules(&rules.read, read))?
			.add_rules(path_beneath_rules(&rules.execute, read | AccessFs::Execute))?
			.add_rules(path_beneath_rules(&rules.write, write))?
			.add_rules(path_beneath_rules(&rules.connect, AccessFs::ResolveUnix))?
			.add_rules(
				TCP_PORTS
					.map(|port| Ok::<_, RulesetError>(NetPort::new(port, AccessNet::ConnectTcp))),
			)?
			.restrict_self()?;
		Ok(status.ruleset)
	}
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
	use std::ffi::OsString;

	use super::{Policy, UNSANDBOXED_ENV};

	/// No sandbox on this platform.
	pub fn exec(_: &Policy, _: Vec<OsString>) -> anyhow::Error {
		anyhow::anyhow!(
			"published workers can only be sandboxed on macOS and Linux; \
			 set {UNSANDBOXED_ENV}=1 to run them without the sandbox"
		)
	}
}

/// The Landlock rules enforcing a policy on Linux. A path the system lacks
/// grants nothing.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandlockRules {
	/// Trees the worker reads but neither writes nor executes from.
	pub read: Vec<PathBuf>,
	/// Files the worker may also execute: `bun` and the dynamic loaders.
	pub execute: Vec<PathBuf>,
	/// Trees the worker may write.
	pub write: Vec<PathBuf>,
	/// The Unix sockets the worker may connect to, on kernels that confine
	/// them.
	pub connect: Vec<PathBuf>,
}

/// What a dynamically linked `bun` reads to start and to reach the network:
/// shared libraries, name resolution, certificates and time zones.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_SYSTEM_READ: &[&str] = &[
	"/lib",
	"/lib64",
	"/usr/lib",
	"/usr/lib64",
	"/etc/ld.so.cache",
	"/etc/ld.so.conf",
	"/etc/ld.so.conf.d",
	"/etc/nsswitch.conf",
	"/etc/host.conf",
	"/etc/hosts",
	"/etc/resolv.conf",
	"/etc/gai.conf",
	"/etc/localtime",
	"/etc/ssl",
	"/etc/pki",
	"/etc/ca-certificates",
	"/usr/share/ca-certificates",
	"/usr/share/zoneinfo",
	"/dev/urandom",
	"/dev/random",
];

/// The dynamic loaders `bun` may be linked against, which the kernel
/// executes alongside it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const LINUX_LOADERS: &[&str] = &[
	"/lib64/ld-linux-x86-64.so.2",
	"/lib/ld-linux-x86-64.so.2",
	"/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
	"/lib/ld-linux-aarch64.so.1",
	"/lib/aarch64-linux-gnu/ld-linux-aarch64.so.1",
	"/lib/ld-musl-x86_64.so.1",
	"/lib/ld-musl-aarch64.so.1",
];

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl LandlockRules {
	/// The rules enforcing `policy`.
	pub fn of(policy: &Policy) -> Self {
		let mut read: Vec<PathBuf> = LINUX_SYSTEM_READ.iter().map(PathBuf::from).collect();
		read.extend(policy.read.iter().cloned());
		let mut execute = vec![policy.program.clone()];
		execute.extend(LINUX_LOADERS.iter().map(PathBuf::from));
		Self {
			read,
			execute,
			write: vec![policy.scratch.clone(), PathBuf::from("/dev/null")],
			connect: vec![policy.socket.clone()],
		}
	}
}

#[cfg(test)]
mod tests;
