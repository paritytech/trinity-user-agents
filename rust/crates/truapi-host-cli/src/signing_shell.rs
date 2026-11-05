//! Slash-command parsing and command-bar editing for the signing-host UI.

use std::{
	fmt, fs,
	path::{Path, PathBuf},
};

use truapi::platform::{ProductId, ProductIdError};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{InvalidLogLevel, LogLevel, sessions};

/// Operation selected through `/product`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductCommand {
	/// Print the currently selected product.
	Current,
	/// Switch to the validated, normalized product id.
	Switch(ProductId),
}

/// Operation selected through `/devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCommand {
	/// List paired devices for the active managed session.
	List,
	/// Notify and remove one device; `force` permits cleanup if notification fails.
	Remove { statement_account_id: [u8; 32], force: bool },
}

/// Operation selected through `/approval`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalCommand {
	/// Print the current approval mode.
	Current,
	/// Prompt for every confirmation.
	Manual,
	/// Approve every confirmation without prompting.
	Automatic,
}

/// Operation selected through `/session`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionCommand {
	/// Print the current session.
	Current,
	/// List sessions for the active network.
	List,
	/// Restore or provision the named session, including unfinished setup.
	Switch(String),
	/// Permanently clear one named session or every session for the network.
	Clear(sessions::SessionClearTarget),
	/// Import an existing signer and initialize its username-owned session.
	ImportMnemonic(SecretMnemonic),
}

/// Pairing input selected through `/pair`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairCommand {
	/// Acquire a pairing deeplink from a copied QR image.
	Scan,
	/// Decode a pairing deeplink from an image file.
	Image(PathBuf),
	/// Answer the supplied Polkadot Mobile pairing deeplink.
	Deeplink(String),
}

/// A mnemonic accepted by the command parser without exposing it through
/// derived debug output or retaining it after the command is dropped.
#[derive(Clone, PartialEq, Eq, Zeroize, ZeroizeOnDrop, derive_more::Debug)]
#[debug("<redacted>")]
pub struct SecretMnemonic(String);

impl SecretMnemonic {
	fn new(value: String) -> Self {
		Self(value)
	}

	/// Borrow the phrase only at the account-import boundary.
	pub fn expose_secret(&self) -> &str {
		&self.0
	}
}

/// Whether a command carries mnemonic material that must not enter UI history,
/// transcripts, busy labels, or diagnostics.
pub fn contains_mnemonic(command: &str) -> bool {
	mnemonic_option_end(command).is_some()
}

fn mnemonic_option_end(command: &str) -> Option<usize> {
	let trimmed = command.trim_start();
	let leading_bytes = command.len().saturating_sub(trimmed.len());
	let after_session = trimmed.strip_prefix("/session")?;
	if !after_session.starts_with(char::is_whitespace) {
		return None;
	}
	let option = after_session.trim_start();
	let option_start = command.len().saturating_sub(option.len());
	let suffix = option.strip_prefix("--mnemonic")?;
	if !suffix.is_empty() && !suffix.starts_with(char::is_whitespace) {
		return None;
	}
	Some(leading_bytes.max(option_start) + "--mnemonic".len())
}

/// Replace mnemonic characters while preserving command length and cursor
/// placement in the interactive command bar.
pub fn mask_mnemonic(command: &str) -> Option<String> {
	if !contains_mnemonic(command) {
		return None;
	}
	let argument_start = mnemonic_option_end(command)?;
	let mut masked = command[..argument_start].to_string();
	masked.extend(
		command[argument_start..]
			.chars()
			.map(|character| if character.is_whitespace() { character } else { '•' }),
	);
	Some(masked)
}

/// Select, edit, or run a product script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptCommand {
	/// Edit the remembered script, creating a project if needed, then run it.
	Edit,
	/// Edit without running the script.
	EditOnly,
	/// Run an explicit path or the remembered script without opening an editor.
	Run(Option<PathBuf>),
	/// Create a project in a new directory, then edit and run its script.
	New(Option<PathBuf>),
}

impl ScriptCommand {
	/// Whether this action needs an interactive editor.
	pub fn edits(&self) -> bool {
		!matches!(self, Self::Run(_))
	}

	/// Whether this action runs the selected script.
	pub fn runs(&self) -> bool {
		!matches!(self, Self::EditOnly)
	}
}

/// A command accepted by the signing-host command bar or `exec` mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellCommand {
	/// Scan or answer a Polkadot Mobile pairing request.
	Pair(PairCommand),
	/// Inspect or remove paired devices for the active managed session.
	Devices(DeviceCommand),
	/// Inspect or change how future confirmations are approved.
	Approval(ApprovalCommand),
	/// Edit the remembered product script, or run an explicit one, through the
	/// public frame endpoint.
	Script(ScriptCommand),
	/// Show command and keyboard help.
	Help,
	/// Clear the visible transcript.
	Clear,
	/// Copy the retained transcript to the system clipboard.
	Copy,
	/// Start the pairing-host login flow for the selected product.
	Login,
	/// Disconnect a pairing host and discard its old pairing keypair.
	Logout,
	/// Replace the active tracing filter with a log level.
	Log(LogLevel),
	/// Inspect or switch the product used by scripts and frame connections.
	Product(ProductCommand),
	/// Inspect, list, or switch the active persistent session.
	Session(SessionCommand),
	/// Renew tracked statement-store allowances for the current period.
	Renew,
	/// Shut down the signing host.
	Quit,
}

/// Rejection of a command or one of its validated arguments.
#[derive(Debug, PartialEq, Eq, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum CommandParseError {
	/// Command syntax does not match its usage.
	#[display("{reason}")]
	#[from(String, &'static str)]
	Usage {
		/// Usage text shown at the command boundary.
		reason: String,
	},
	/// The selected session is not a valid user profile.
	#[from]
	Session(sessions::SessionNameError),
	/// The log filter names an unsupported level.
	#[from]
	LogLevel(InvalidLogLevel),
	/// The product cannot be used as a protocol identifier.
	#[from]
	Product(ProductIdError),
}

/// Parse one slash command without invoking a shell.
pub fn parse_command(input: &str) -> Result<ShellCommand, CommandParseError> {
	let input = input.trim();
	if input.is_empty() {
		return Err("enter a command starting with `/`".into());
	}
	if !input.starts_with('/') {
		return Err("commands start with `/`; use /help to list them".into());
	}

	let (name, argument) = input
		.split_once(char::is_whitespace)
		.map_or((input, ""), |(name, argument)| (name, argument.trim()));
	match name {
		"/pair" => {
			if argument.is_empty() {
				return Ok(ShellCommand::Pair(PairCommand::Scan));
			}
			if argument.starts_with("polkadotapp://pair?") {
				return Ok(ShellCommand::Pair(PairCommand::Deeplink(argument.to_string())));
			}
			let arguments = shlex::split(argument).ok_or("invalid /pair image path quoting")?;
			if arguments.len() != 1 || argument.contains("://") {
				return Err("usage: /pair [<image-path> | <polkadotapp://pair?...>]".into());
			}
			Ok(ShellCommand::Pair(PairCommand::Image(PathBuf::from(&arguments[0]))))
		},
		"/devices" => {
			if argument.is_empty() || argument == "--list" {
				return Ok(ShellCommand::Devices(DeviceCommand::List));
			}
			let arguments = shlex::split(argument).ok_or("invalid /devices quoting")?;
			if arguments.first().is_some_and(|value| value == "--remove") {
				let (statement_account_id, force) = match arguments.as_slice() {
					[_, statement_account_id] => (statement_account_id, false),
					[_, statement_account_id, force] if force == "--force" => {
						(statement_account_id, true)
					},
					_ => {
						return Err(
							"usage: /devices --remove <statement-account-id> [--force]".into()
						);
					},
				};
				return Ok(ShellCommand::Devices(DeviceCommand::Remove {
					statement_account_id: parse_statement_account_id(statement_account_id)?,
					force,
				}));
			}
			Err("usage: /devices [--list | --remove <statement-account-id> [--force]]".into())
		},
		"/approval" => match argument {
			"" => Ok(ShellCommand::Approval(ApprovalCommand::Current)),
			"manual" => Ok(ShellCommand::Approval(ApprovalCommand::Manual)),
			"automatic" => Ok(ShellCommand::Approval(ApprovalCommand::Automatic)),
			_ => Err("usage: /approval [manual|automatic]".into()),
		},
		"/script" => {
			let command = match argument {
				"" => ScriptCommand::Edit,
				"--run" => ScriptCommand::Run(None),
				"--edit" => ScriptCommand::EditOnly,
				"--new" => ScriptCommand::New(None),
				"--" => return Err("usage: /script -- <path>".into()),
				_ => match argument.split_once(char::is_whitespace) {
					Some(("--new", directory)) => {
						ScriptCommand::New(Some(PathBuf::from(directory.trim())))
					},
					Some(("--", path)) => ScriptCommand::Run(Some(PathBuf::from(path.trim()))),
					_ => ScriptCommand::Run(Some(PathBuf::from(argument))),
				},
			};
			Ok(ShellCommand::Script(command))
		},
		"/help" => no_argument(name, argument, ShellCommand::Help),
		"/clear" => no_argument(name, argument, ShellCommand::Clear),
		"/copy" => no_argument(name, argument, ShellCommand::Copy),
		"/login" => no_argument(name, argument, ShellCommand::Login),
		"/logout" => no_argument(name, argument, ShellCommand::Logout),
		"/log" => {
			if argument.is_empty() {
				return Err("usage: /log <error|warn|info|debug|trace>".into());
			}
			Ok(ShellCommand::Log(argument.parse::<LogLevel>()?))
		},
		"/product" => {
			if argument.is_empty() {
				return Ok(ShellCommand::Product(ProductCommand::Current));
			}
			let product_id = argument.parse::<ProductId>()?;
			Ok(ShellCommand::Product(ProductCommand::Switch(product_id)))
		},
		"/session" => {
			if argument.is_empty() {
				return Ok(ShellCommand::Session(SessionCommand::Current));
			}
			let arguments = shlex::split(argument)
				.ok_or("invalid /session quoting; close the mnemonic quote")?;
			let first = arguments.first().expect("non-empty session argument");
			match first.as_str() {
				"--list" if arguments.len() == 1 => {
					return Ok(ShellCommand::Session(SessionCommand::List));
				},
				"--clear" => {
					let name = arguments.get(1).ok_or("usage: /session --clear <name>")?;
					if arguments.len() != 2 {
						return Err("usage: /session --clear <name>".into());
					}
					sessions::validate_selectable_name(name)?;
					return Ok(ShellCommand::Session(SessionCommand::Clear(
						sessions::SessionClearTarget::Named(name.to_string()),
					)));
				},
				"--clear-all" if arguments.len() == 1 => {
					return Ok(ShellCommand::Session(SessionCommand::Clear(
						sessions::SessionClearTarget::All,
					)));
				},
				"--mnemonic" => {
					if arguments.len() < 2 {
						return Err("usage: /session --mnemonic \"<BIP-39 phrase>\"".into());
					}
					return Ok(ShellCommand::Session(SessionCommand::ImportMnemonic(
						SecretMnemonic::new(arguments[1..].join(" ")),
					)));
				},
				option if option.starts_with("--") => {
					return Err(format!(
						"unknown /session option `{option}`; use /help to list options"
					)
					.into());
				},
				_ if arguments.len() != 1 => {
					return Err("usage: /session <name>".into());
				},
				_ => {},
			}
			sessions::validate_selectable_name(first)?;
			Ok(ShellCommand::Session(SessionCommand::Switch(first.to_string())))
		},
		"/renew" => no_argument(name, argument, ShellCommand::Renew),
		"/quit" => no_argument(name, argument, ShellCommand::Quit),
		_ => Err(format!("unknown command `{name}`; use /help to list commands").into()),
	}
}

fn parse_statement_account_id(value: &str) -> Result<[u8; 32], CommandParseError> {
	let value = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")).unwrap_or(value);
	let bytes = hex::decode(value).map_err(|_| {
		CommandParseError::from("statement account ID must be 32 bytes of hexadecimal")
	})?;
	bytes.try_into().map_err(|_| {
		CommandParseError::from("statement account ID must be 32 bytes of hexadecimal")
	})
}

fn no_argument(
	name: &str,
	argument: &str,
	command: ShellCommand,
) -> Result<ShellCommand, CommandParseError> {
	if argument.is_empty() {
		Ok(command)
	} else {
		Err(format!("{name} does not accept arguments").into())
	}
}

/// One selectable completion shown above the command bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
	/// Complete input inserted into the command bar.
	pub value: String,
	/// Short description rendered beside the completion.
	pub description: &'static str,
}

const SIGNING_COMMANDS: &[(&str, &str)] = &[
	("/pair", "paste a pairing QR image or provide a file or URL"),
	("/devices", "list or remove paired devices"),
	("/approval", "show or change confirmation approval"),
	("/script", "edit the last or run an existing product script"),
	("/log", "set error, warn, info, debug, or trace"),
	("/product", "show or switch the active product"),
	("/session", "show, switch, or clear sessions"),
	("/renew", "renew statement-store allowances now"),
	("/help", "show commands and keyboard shortcuts"),
	("/clear", "clear the visible transcript"),
	("/copy", "copy the transcript to the clipboard"),
	("/quit", "shut down the signing host"),
];

const PAIRING_COMMANDS: &[(&str, &str)] = &[
	("/script", "edit the last or run an existing product script"),
	("/login", "pair with a signing host"),
	("/logout", "disconnect and reset pairing keys"),
	("/log", "set error, warn, info, debug, or trace"),
	("/product", "show or switch the active product"),
	("/help", "show commands and keyboard shortcuts"),
	("/clear", "clear the visible transcript"),
	("/copy", "copy the transcript to the clipboard"),
	("/quit", "shut down the pairing host"),
];

const LOG_ARGUMENTS: &[(&str, &str)] = &[
	("error", "show only errors"),
	("warn", "show warnings and errors"),
	("info", "show informational host activity"),
	("debug", "show detailed host activity"),
	("trace", "show all host and protocol activity"),
];

const APPROVAL_ARGUMENTS: &[(&str, &str)] = &[
	("manual", "prompt for every confirmation"),
	("automatic", "approve confirmations automatically"),
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CommandScope {
	PairingHost,
	#[default]
	SigningHost,
}

fn completions_for_scope(
	input: &str,
	session_names: &[String],
	scope: CommandScope,
) -> Vec<Completion> {
	if let Some(path) = input.strip_prefix("/script ") {
		if let Some(path) = path.strip_prefix("--new ") {
			return path_completions(path, "/script --new");
		}
		if let Some(path) = path.strip_prefix("-- ") {
			return path_completions(path, "/script --");
		}
		let mut completions = fixed_argument_completions(
			"/script",
			path,
			&[
				("--run", "rerun the remembered script"),
				("--edit", "edit without running"),
				("--new", "create a new project"),
			],
		);
		completions.extend(path_completions(path, "/script"));
		return completions;
	}
	if let Some(prefix) = input.strip_prefix("/log ") {
		return fixed_argument_completions("/log", prefix, LOG_ARGUMENTS);
	}
	if scope == CommandScope::SigningHost &&
		let Some(prefix) = input.strip_prefix("/approval ")
	{
		return fixed_argument_completions("/approval", prefix, APPROVAL_ARGUMENTS);
	}
	if scope == CommandScope::SigningHost &&
		let Some(prefix) = input.strip_prefix("/devices ")
	{
		if let Some((statement_account_id, force_prefix)) = prefix
			.strip_prefix("--remove ")
			.and_then(|value| value.split_once(char::is_whitespace))
		{
			return fixed_argument_completions(
				&format!("/devices --remove {statement_account_id}"),
				force_prefix.trim_start(),
				&[("--force", "remove locally if notification fails")],
			);
		}
		return fixed_argument_completions(
			"/devices",
			prefix,
			&[("--list", "list paired devices"), ("--remove", "remove one paired device")],
		);
	}
	if scope == CommandScope::SigningHost &&
		let Some(prefix) = input.strip_prefix("/session --clear ")
	{
		if prefix.contains(char::is_whitespace) {
			return Vec::new();
		}
		return session_names
			.iter()
			.filter(|name| name.starts_with(prefix))
			.map(|name| Completion {
				value: format!("/session --clear {name}"),
				description: "clear existing session",
			})
			.collect();
	}
	if scope == CommandScope::SigningHost &&
		let Some(prefix) = input.strip_prefix("/session ")
	{
		if prefix.contains(char::is_whitespace) {
			return Vec::new();
		}
		let mut matches = session_names
			.iter()
			.filter(|name| name.starts_with(prefix))
			.map(|name| Completion {
				value: format!("/session {name}"),
				description: "existing session",
			})
			.collect::<Vec<_>>();
		if "--list".starts_with(prefix) {
			matches.insert(
				0,
				Completion { value: "/session --list".to_string(), description: "list sessions" },
			);
		}
		for (value, description) in [
			("--mnemonic", "import an existing signer"),
			("--clear", "clear one session"),
			("--clear-all", "clear all sessions"),
		] {
			if value.starts_with(prefix) {
				matches.push(Completion { value: format!("/session {value}"), description });
			}
		}
		return matches;
	}
	if !input.starts_with('/') || input.contains(char::is_whitespace) {
		return Vec::new();
	}
	let commands = match scope {
		CommandScope::PairingHost => PAIRING_COMMANDS,
		CommandScope::SigningHost => SIGNING_COMMANDS,
	};
	commands
		.iter()
		.filter(|(command, _)| command.trim_end().starts_with(input))
		.map(|(command, description)| Completion { value: (*command).to_string(), description })
		.collect()
}

fn fixed_argument_completions(
	command: &str,
	prefix: &str,
	arguments: &[(&str, &'static str)],
) -> Vec<Completion> {
	if prefix.contains(char::is_whitespace) {
		return Vec::new();
	}
	arguments
		.iter()
		.filter(|(argument, _)| argument.starts_with(prefix))
		.map(|(argument, description)| Completion {
			value: format!("{command} {argument}"),
			description,
		})
		.collect()
}

fn path_completions(input: &str, command: &str) -> Vec<Completion> {
	let path = Path::new(input);
	let ends_with_separator = input.ends_with(std::path::MAIN_SEPARATOR);
	let (directory, prefix) = if ends_with_separator {
		(path, "")
	} else {
		(
			path.parent().unwrap_or_else(|| Path::new(".")),
			path.file_name().and_then(|name| name.to_str()).unwrap_or(""),
		)
	};
	let Ok(entries) = fs::read_dir(directory) else {
		return Vec::new();
	};
	let displayed_parent = if ends_with_separator {
		input.to_string()
	} else {
		input.strip_suffix(prefix).unwrap_or_default().to_string()
	};
	let mut matches = entries
		.filter_map(Result::ok)
		.filter_map(|entry| {
			let name = entry.file_name().into_string().ok()?;
			if !name.starts_with(prefix) {
				return None;
			}
			let suffix = if entry.file_type().ok()?.is_dir() { "/" } else { "" };
			Some(Completion {
				value: format!("{command} {displayed_parent}{name}{suffix}"),
				description: "filesystem path",
			})
		})
		.collect::<Vec<_>>();
	matches.sort_by(|left, right| left.value.cmp(&right.value));
	matches.truncate(8);
	matches
}

/// Editable command input with completion selection and in-memory history.
#[derive(Default)]
pub struct CommandEditor {
	chars: Vec<char>,
	cursor: usize,
	history: Vec<String>,
	history_index: Option<usize>,
	history_draft: String,
	completion_index: usize,
	completions_dismissed: bool,
	session_names: Vec<String>,
	scope: CommandScope,
}

impl fmt::Debug for CommandEditor {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let text = self.text();
		let text = if contains_mnemonic(&text) { "<redacted>".to_string() } else { text };
		f.debug_struct("CommandEditor")
			.field("text", &text)
			.field("cursor", &self.cursor)
			.field("history_entries", &self.history.len())
			.field("scope", &self.scope)
			.finish()
	}
}

impl Drop for CommandEditor {
	fn drop(&mut self) {
		self.chars.zeroize();
		self.history.zeroize();
		self.history_draft.zeroize();
	}
}

impl CommandEditor {
	/// Build an editor exposing only commands supported by the pairing host.
	pub fn pairing_host() -> Self {
		let mut editor = Self::default();
		editor.scope = CommandScope::PairingHost;
		editor
	}

	/// Return the current command text.
	pub fn text(&self) -> String {
		self.chars.iter().collect()
	}

	/// Return the character-indexed cursor position.
	pub fn cursor(&self) -> usize {
		self.cursor
	}

	/// Replace the command text and place the cursor at its end.
	pub fn set_text(&mut self, value: impl Into<String>) {
		self.chars.zeroize();
		self.chars = value.into().chars().collect();
		self.cursor = self.chars.len();
		self.edited();
	}

	/// Insert one character at the cursor.
	pub fn insert(&mut self, value: char) {
		self.chars.insert(self.cursor, value);
		self.cursor += 1;
		self.edited();
	}

	/// Remove the character before the cursor.
	pub fn backspace(&mut self) {
		if self.cursor > 0 {
			self.cursor -= 1;
			self.chars.remove(self.cursor);
			self.edited();
		}
	}

	/// Remove the character under the cursor.
	pub fn delete(&mut self) {
		if self.cursor < self.chars.len() {
			self.chars.remove(self.cursor);
			self.edited();
		}
	}

	/// Move the cursor one character left.
	pub fn left(&mut self) {
		self.cursor = self.cursor.saturating_sub(1);
	}

	/// Move the cursor one character right.
	pub fn right(&mut self) {
		self.cursor = (self.cursor + 1).min(self.chars.len());
	}

	/// Move the cursor to the start of the input.
	pub fn home(&mut self) {
		self.cursor = 0;
	}

	/// Move the cursor to the end of the input.
	pub fn end(&mut self) {
		self.cursor = self.chars.len();
	}

	/// Clear the current command without adding it to history.
	pub fn clear(&mut self) {
		self.chars.zeroize();
		self.chars.clear();
		self.cursor = 0;
		self.edited();
	}

	/// Replace session names offered after `/session `.
	pub fn set_session_names(&mut self, names: Vec<String>) {
		self.session_names = names;
		self.edited();
	}

	/// Return currently visible completions.
	pub fn completions(&self) -> Vec<Completion> {
		if self.completions_dismissed {
			Vec::new()
		} else {
			completions_for_scope(&self.text(), &self.session_names, self.scope)
		}
	}

	/// Return the selected completion index, clamped to visible results.
	pub fn completion_index(&self) -> usize {
		self.completion_index.min(self.completions().len().saturating_sub(1))
	}

	/// Move to the previous completion, or older history when none is shown.
	pub fn up(&mut self) {
		let completions = self.completions();
		if !completions.is_empty() {
			self.completion_index =
				self.completion_index().checked_sub(1).unwrap_or(completions.len() - 1);
			return;
		}
		self.older_history();
	}

	/// Move to the next completion, or newer history when none is shown.
	pub fn down(&mut self) {
		let completions = self.completions();
		if !completions.is_empty() {
			self.completion_index = (self.completion_index() + 1) % completions.len();
			return;
		}
		self.newer_history();
	}

	/// Insert the selected completion, returning whether one existed.
	pub fn accept_completion(&mut self) -> bool {
		let completions = self.completions();
		let Some(completion) = completions.get(self.completion_index()) else {
			return false;
		};
		self.set_text(completion.value.clone());
		true
	}

	/// Hide completions until the command text changes.
	pub fn dismiss_completions(&mut self) {
		self.completions_dismissed = true;
	}

	/// Submit and remember the current input, clearing the editor.
	pub fn submit(&mut self) -> String {
		let value = self.text();
		if !value.trim().is_empty() &&
			!contains_mnemonic(&value) &&
			self.history.last() != Some(&value)
		{
			self.history.push(value.clone());
		}
		self.chars.zeroize();
		self.chars.clear();
		self.cursor = 0;
		self.history_index = None;
		self.history_draft.clear();
		self.edited();
		value
	}

	fn edited(&mut self) {
		self.completion_index = 0;
		self.completions_dismissed = false;
		self.history_index = None;
	}

	fn older_history(&mut self) {
		if self.history.is_empty() {
			return;
		}
		let index = match self.history_index {
			Some(index) => index.saturating_sub(1),
			None => {
				self.history_draft.zeroize();
				let draft = self.text();
				if !contains_mnemonic(&draft) {
					self.history_draft = draft;
				}
				self.history.len() - 1
			},
		};
		self.history_index = Some(index);
		self.chars.zeroize();
		self.chars = self.history[index].chars().collect();
		self.cursor = self.chars.len();
	}

	fn newer_history(&mut self) {
		let Some(index) = self.history_index else {
			return;
		};
		if index + 1 < self.history.len() {
			let next = index + 1;
			self.history_index = Some(next);
			self.chars.zeroize();
			self.chars = self.history[next].chars().collect();
		} else {
			self.history_index = None;
			self.chars.zeroize();
			self.chars = self.history_draft.chars().collect();
		}
		self.cursor = self.chars.len();
	}
}

/// Parse a confirmation answer, returning `None` for invalid input.
pub fn parse_approval(input: &str) -> Option<bool> {
	match input.trim().to_ascii_lowercase().as_str() {
		"y" | "yes" => Some(true),
		"n" | "no" => Some(false),
		_ => None,
	}
}

/// Text displayed by `/help` in either presentation mode.
pub const HELP_TEXT: &str = "\
/pair                   paste or drop a pairing QR image
/pair <image-path>      read a pairing QR image file
/pair <url>             answer a Polkadot Mobile pairing URL
/devices                list paired devices for the active session
/devices --remove <id>  disconnect and remove one paired device by statement account ID
/devices --remove <id> --force  remove locally even if notification fails
/approval               show the current confirmation approval mode
/approval manual        prompt for every future confirmation
/approval automatic     approve every future confirmation automatically
/script                 edit and run the session's last Bun TypeScript script
/script <path>          run an existing JS/TS product script with Bun
/script --run           rerun the remembered script
/script --edit          edit without running
/script --new [dir]     create and edit a new project
/log <level>            set error, warn, info, debug, or trace
/product                show the current product
/product <id>           switch product and reconnect product clients
/session                show the current session and path
/session <name>         restore or provision a session; retry unfinished setup
/session --mnemonic \"<phrase>\" import an existing signer as its username session
/session --list         list sessions for this network
/session --clear <name> permanently clear one session
/session --clear-all    permanently clear all sessions for this network
/renew                  renew statement-store allowances now
/help                   show this help
/clear                  clear the visible transcript
/copy                   copy the transcript to the clipboard
/quit                   shut down the signing host

Keys: Ctrl-V or terminal paste for a pairing image, Up/Down completion or history, Tab complete, Ctrl-U/Ctrl-D scroll,
Esc close completion or reject approval, Ctrl-C clear/cancel/quit";

/// Help shown by the pairing-host command bar.
pub const PAIRING_HELP_TEXT: &str = "\
/script                 edit and run the last Bun TypeScript product script
/script <path>          run an existing JS/TS product script with Bun
/script --run           rerun the remembered script
/script --edit          edit without running
/script --new [dir]     create and edit a new project
/login                  pair with a signing host for the current product
/logout                 disconnect and reset pairing keys
/log <level>            set error, warn, info, debug, or trace
/product                show the current product
/product <id>           switch product and reconnect product clients
/help                   show this help
/clear                  clear the visible transcript
/copy                   copy the transcript to the clipboard
/quit                   shut down the pairing host

Keys: Up/Down completion or history, Tab complete, Ctrl-U/Ctrl-D scroll,
Esc close completion, Ctrl-C clear/cancel/quit";

#[cfg(test)]
mod tests;
