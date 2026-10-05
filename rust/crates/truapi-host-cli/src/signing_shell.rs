//! Slash-command parsing and command-bar editing for the signing-host UI.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use truapi::platform::normalize_product_identifier;
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::LogLevel;
use crate::sessions;

/// Operation selected through `/product`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProductCommand {
    /// Print the currently selected product.
    Current,
    /// Switch to the validated, normalized product id.
    Switch(String),
}

/// Operation selected through `/devices`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceCommand {
    /// List paired devices for the active managed session.
    List,
    /// Notify and remove one device; `force` permits cleanup if notification fails.
    Remove {
        statement_account_id: [u8; 32],
        force: bool,
    },
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
    masked.extend(command[argument_start..].chars().map(|character| {
        if character.is_whitespace() {
            character
        } else {
            '•'
        }
    }));
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

/// Parse one slash command without invoking a shell.
pub fn parse_command(input: &str) -> Result<ShellCommand, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("enter a command starting with `/`".to_string());
    }
    if !input.starts_with('/') {
        return Err("commands start with `/`; use /help to list them".to_string());
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
                return Ok(ShellCommand::Pair(PairCommand::Deeplink(
                    argument.to_string(),
                )));
            }
            let arguments = shlex::split(argument)
                .ok_or_else(|| "invalid /pair image path quoting".to_string())?;
            if arguments.len() != 1 || argument.contains("://") {
                return Err("usage: /pair [<image-path> | <polkadotapp://pair?...>]".to_string());
            }
            Ok(ShellCommand::Pair(PairCommand::Image(PathBuf::from(
                &arguments[0],
            ))))
        }
        "/devices" => {
            if argument.is_empty() || argument == "--list" {
                return Ok(ShellCommand::Devices(DeviceCommand::List));
            }
            let arguments =
                shlex::split(argument).ok_or_else(|| "invalid /devices quoting".to_string())?;
            if arguments.first().is_some_and(|value| value == "--remove") {
                let (statement_account_id, force) = match arguments.as_slice() {
                    [_, statement_account_id] => (statement_account_id, false),
                    [_, statement_account_id, force] if force == "--force" => {
                        (statement_account_id, true)
                    }
                    _ => {
                        return Err(
                            "usage: /devices --remove <statement-account-id> [--force]".to_string()
                        );
                    }
                };
                return Ok(ShellCommand::Devices(DeviceCommand::Remove {
                    statement_account_id: parse_statement_account_id(statement_account_id)?,
                    force,
                }));
            }
            Err("usage: /devices [--list | --remove <statement-account-id> [--force]]".to_string())
        }
        "/approval" => match argument {
            "" => Ok(ShellCommand::Approval(ApprovalCommand::Current)),
            "manual" => Ok(ShellCommand::Approval(ApprovalCommand::Manual)),
            "automatic" => Ok(ShellCommand::Approval(ApprovalCommand::Automatic)),
            _ => Err("usage: /approval [manual|automatic]".to_string()),
        },
        "/script" => {
            let command = match argument {
                "" => ScriptCommand::Edit,
                "--run" => ScriptCommand::Run(None),
                "--edit" => ScriptCommand::EditOnly,
                "--new" => ScriptCommand::New(None),
                "--" => return Err("usage: /script -- <path>".to_string()),
                _ => match argument.split_once(char::is_whitespace) {
                    Some(("--new", directory)) => {
                        ScriptCommand::New(Some(PathBuf::from(directory.trim())))
                    }
                    Some(("--", path)) => ScriptCommand::Run(Some(PathBuf::from(path.trim()))),
                    _ => ScriptCommand::Run(Some(PathBuf::from(argument))),
                },
            };
            Ok(ShellCommand::Script(command))
        }
        "/help" => no_argument(name, argument, ShellCommand::Help),
        "/clear" => no_argument(name, argument, ShellCommand::Clear),
        "/copy" => no_argument(name, argument, ShellCommand::Copy),
        "/login" => no_argument(name, argument, ShellCommand::Login),
        "/logout" => no_argument(name, argument, ShellCommand::Logout),
        "/log" => {
            if argument.is_empty() {
                return Err("usage: /log <error|warn|info|debug|trace>".to_string());
            }
            Ok(ShellCommand::Log(argument.parse::<LogLevel>()?))
        }
        "/product" => {
            if argument.is_empty() {
                return Ok(ShellCommand::Product(ProductCommand::Current));
            }
            let product_id =
                normalize_product_identifier(argument).map_err(|error| error.to_string())?;
            Ok(ShellCommand::Product(ProductCommand::Switch(product_id)))
        }
        "/session" => {
            if argument.is_empty() {
                return Ok(ShellCommand::Session(SessionCommand::Current));
            }
            let arguments = shlex::split(argument)
                .ok_or_else(|| "invalid /session quoting; close the mnemonic quote".to_string())?;
            let first = arguments.first().expect("non-empty session argument");
            match first.as_str() {
                "--list" if arguments.len() == 1 => {
                    return Ok(ShellCommand::Session(SessionCommand::List));
                }
                "--clear" => {
                    let Some(name) = arguments.get(1) else {
                        return Err("usage: /session --clear <name>".to_string());
                    };
                    if arguments.len() != 2 {
                        return Err("usage: /session --clear <name>".to_string());
                    }
                    sessions::validate_selectable_name(name)?;
                    return Ok(ShellCommand::Session(SessionCommand::Clear(
                        sessions::SessionClearTarget::Named(name.to_string()),
                    )));
                }
                "--clear-all" if arguments.len() == 1 => {
                    return Ok(ShellCommand::Session(SessionCommand::Clear(
                        sessions::SessionClearTarget::All,
                    )));
                }
                "--mnemonic" => {
                    if arguments.len() < 2 {
                        return Err("usage: /session --mnemonic \"<BIP-39 phrase>\"".to_string());
                    }
                    return Ok(ShellCommand::Session(SessionCommand::ImportMnemonic(
                        SecretMnemonic::new(arguments[1..].join(" ")),
                    )));
                }
                option if option.starts_with("--") => {
                    return Err(format!(
                        "unknown /session option `{option}`; use /help to list options"
                    ));
                }
                _ if arguments.len() != 1 => {
                    return Err("usage: /session <name>".to_string());
                }
                _ => {}
            }
            sessions::validate_selectable_name(first)?;
            Ok(ShellCommand::Session(SessionCommand::Switch(
                first.to_string(),
            )))
        }
        "/renew" => no_argument(name, argument, ShellCommand::Renew),
        "/quit" => no_argument(name, argument, ShellCommand::Quit),
        _ => Err(format!(
            "unknown command `{name}`; use /help to list commands"
        )),
    }
}

fn parse_statement_account_id(value: &str) -> Result<[u8; 32], String> {
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    let bytes = hex::decode(value)
        .map_err(|_| "statement account ID must be 32 bytes of hexadecimal".to_string())?;
    bytes
        .try_into()
        .map_err(|_| "statement account ID must be 32 bytes of hexadecimal".to_string())
}

fn no_argument(name: &str, argument: &str, command: ShellCommand) -> Result<ShellCommand, String> {
    if argument.is_empty() {
        Ok(command)
    } else {
        Err(format!("{name} does not accept arguments"))
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
    if scope == CommandScope::SigningHost
        && let Some(prefix) = input.strip_prefix("/approval ")
    {
        return fixed_argument_completions("/approval", prefix, APPROVAL_ARGUMENTS);
    }
    if scope == CommandScope::SigningHost
        && let Some(prefix) = input.strip_prefix("/devices ")
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
            &[
                ("--list", "list paired devices"),
                ("--remove", "remove one paired device"),
            ],
        );
    }
    if scope == CommandScope::SigningHost
        && let Some(prefix) = input.strip_prefix("/session --clear ")
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
    if scope == CommandScope::SigningHost
        && let Some(prefix) = input.strip_prefix("/session ")
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
                Completion {
                    value: "/session --list".to_string(),
                    description: "list sessions",
                },
            );
        }
        for (value, description) in [
            ("--mnemonic", "import an existing signer"),
            ("--clear", "clear one session"),
            ("--clear-all", "clear all sessions"),
        ] {
            if value.starts_with(prefix) {
                matches.push(Completion {
                    value: format!("/session {value}"),
                    description,
                });
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
        .map(|(command, description)| Completion {
            value: (*command).to_string(),
            description,
        })
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
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(""),
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
            let suffix = if entry.file_type().ok()?.is_dir() {
                "/"
            } else {
                ""
            };
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
        let text = if contains_mnemonic(&text) {
            "<redacted>".to_string()
        } else {
            text
        };
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
        self.completion_index
            .min(self.completions().len().saturating_sub(1))
    }

    /// Move to the previous completion, or older history when none is shown.
    pub fn up(&mut self) {
        let completions = self.completions();
        if !completions.is_empty() {
            self.completion_index = self
                .completion_index()
                .checked_sub(1)
                .unwrap_or(completions.len() - 1);
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
        if !value.trim().is_empty()
            && !contains_mnemonic(&value)
            && self.history.last() != Some(&value)
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
            }
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
mod tests {
    use super::*;

    const MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const DEVICE_ID: &str = "0101010101010101010101010101010101010101010101010101010101010101";

    #[test]
    fn script_actions_preserve_paths_with_spaces_and_option_like_filenames() {
        let inputs = [
            "/script",
            "/script --edit",
            "/script --run",
            "/script --new",
            "/script --new projects/my example",
            "/script projects/my example/script.ts",
            "/script -- --run",
            "/script --other",
            "/script --new-example.ts",
        ];

        assert_eq!(
            inputs.map(parse_command),
            [
                ScriptCommand::Edit,
                ScriptCommand::EditOnly,
                ScriptCommand::Run(None),
                ScriptCommand::New(None),
                ScriptCommand::New(Some(PathBuf::from("projects/my example"))),
                ScriptCommand::Run(Some(PathBuf::from("projects/my example/script.ts"))),
                ScriptCommand::Run(Some(PathBuf::from("--run"))),
                ScriptCommand::Run(Some(PathBuf::from("--other"))),
                ScriptCommand::Run(Some(PathBuf::from("--new-example.ts"))),
            ]
            .map(|command| Ok(ShellCommand::Script(command)))
        );
    }

    #[test]
    fn parses_all_operational_commands() {
        assert_eq!(
            parse_command("/pair"),
            Ok(ShellCommand::Pair(PairCommand::Scan))
        );
        assert_eq!(
            parse_command("/pair polkadotapp://pair?handshake=01"),
            Ok(ShellCommand::Pair(PairCommand::Deeplink(
                "polkadotapp://pair?handshake=01".to_string()
            )))
        );
        assert_eq!(
            parse_command(r#"/pair "/tmp/pairing QR.png""#),
            Ok(ShellCommand::Pair(PairCommand::Image(PathBuf::from(
                "/tmp/pairing QR.png"
            ))))
        );
        assert_eq!(
            parse_command("/script scripts/my smoke.ts"),
            Ok(ShellCommand::Script(ScriptCommand::Run(Some(
                PathBuf::from("scripts/my smoke.ts")
            ))))
        );
        assert_eq!(
            parse_command("/script"),
            Ok(ShellCommand::Script(ScriptCommand::Edit))
        );
        assert_eq!(parse_command("/login"), Ok(ShellCommand::Login));
        assert_eq!(parse_command("/logout"), Ok(ShellCommand::Logout));
        assert_eq!(
            parse_command("/log trace"),
            Ok(ShellCommand::Log(LogLevel::Trace))
        );
        assert_eq!(
            parse_command("/product"),
            Ok(ShellCommand::Product(ProductCommand::Current))
        );
        assert_eq!(
            parse_command("/product Dotli.DOT"),
            Ok(ShellCommand::Product(ProductCommand::Switch(
                "dotli.dot".to_string()
            )))
        );
        assert_eq!(parse_command("/copy"), Ok(ShellCommand::Copy));
        assert_eq!(parse_command("/renew"), Ok(ShellCommand::Renew));
        assert_eq!(
            parse_command("/devices"),
            Ok(ShellCommand::Devices(DeviceCommand::List))
        );
        assert_eq!(
            parse_command(&format!("/devices --remove 0x{DEVICE_ID}")),
            Ok(ShellCommand::Devices(DeviceCommand::Remove {
                statement_account_id: [1; 32],
                force: false,
            }))
        );
        assert_eq!(
            parse_command(&format!("/devices --remove 0X{DEVICE_ID}")),
            Ok(ShellCommand::Devices(DeviceCommand::Remove {
                statement_account_id: [1; 32],
                force: false,
            }))
        );
        assert_eq!(
            parse_command(&format!("/devices --remove 0x{DEVICE_ID} --force")),
            Ok(ShellCommand::Devices(DeviceCommand::Remove {
                statement_account_id: [1; 32],
                force: true,
            }))
        );
        assert_eq!(
            parse_command("/session"),
            Ok(ShellCommand::Session(SessionCommand::Current))
        );
        assert_eq!(
            parse_command("/session alice"),
            Ok(ShellCommand::Session(SessionCommand::Switch(
                "alice".to_string()
            )))
        );
        assert_eq!(
            parse_command("/session --clear alice"),
            Ok(ShellCommand::Session(SessionCommand::Clear(
                sessions::SessionClearTarget::Named("alice".to_string())
            )))
        );
        assert_eq!(
            parse_command("/session --clear-all"),
            Ok(ShellCommand::Session(SessionCommand::Clear(
                sessions::SessionClearTarget::All
            )))
        );
        let imported = parse_command(&format!("/session --mnemonic \"{MNEMONIC}\""))
            .expect("mnemonic command parses");
        let ShellCommand::Session(SessionCommand::ImportMnemonic(mnemonic)) = imported else {
            panic!("unexpected mnemonic command")
        };
        assert_eq!(mnemonic.expose_secret(), MNEMONIC);
    }

    #[test]
    fn parses_runtime_approval_commands() {
        assert_eq!(
            parse_command("/approval"),
            Ok(ShellCommand::Approval(ApprovalCommand::Current))
        );
        assert_eq!(
            parse_command("/approval manual"),
            Ok(ShellCommand::Approval(ApprovalCommand::Manual))
        );
        assert_eq!(
            parse_command("/approval automatic"),
            Ok(ShellCommand::Approval(ApprovalCommand::Automatic))
        );
        assert_eq!(
            parse_command("/approval sometimes").unwrap_err(),
            "usage: /approval [manual|automatic]"
        );
    }

    #[test]
    fn rejects_bare_and_malformed_commands() {
        assert!(
            parse_command("whoami")
                .unwrap_err()
                .contains("start with `/`")
        );
        assert!(parse_command("/whoami").is_err());
        assert!(parse_command("/copy now").is_err());
        assert!(parse_command("/login now").is_err());
        assert!(parse_command("/logout now").is_err());
        assert!(parse_command("/pair https://example.com").is_err());
        assert!(parse_command("/deeplink polkadotapp://pair?handshake=01").is_err());
        assert!(parse_command("/renew now").is_err());
        assert!(parse_command("/devices --remove").is_err());
        assert!(parse_command("/devices --remove not-an-account").is_err());
        assert!(parse_command(&format!("/devices --remove {DEVICE_ID} extra")).is_err());
        assert!(parse_command(&format!("/devices --remove --force {DEVICE_ID}")).is_err());
        assert!(parse_command(&format!("/devices --remove {DEVICE_ID} --force --force")).is_err());
        assert!(parse_command("/devices --force").is_err());
        assert!(parse_command("/devices --unknown").is_err());
        assert!(parse_command("/log noisy").is_err());
        assert!(parse_command("/product example.com").is_err());
        assert!(parse_command("/session ../escape").is_err());
        assert_eq!(
            parse_command("/session --clear").unwrap_err(),
            "usage: /session --clear <name>"
        );
        assert!(parse_command("/session --clear alice bob").is_err());
        assert!(parse_command("/session --clear-all now").is_err());
        assert!(parse_command("/session --unknown").is_err());
        assert!(parse_command("/session --mnemonic").is_err());
        assert!(parse_command("/session --mnemonic \"not closed").is_err());
        assert!(
            parse_command("/session default")
                .unwrap_err()
                .contains("reserved for bootstrap state")
        );
    }

    #[test]
    fn completion_selection_and_history_have_distinct_arrow_behavior() {
        let mut editor = CommandEditor::default();
        editor.set_text("/");
        let first = editor.completions()[0].value.clone();
        editor.down();
        assert_ne!(editor.completions()[editor.completion_index()].value, first);

        editor.dismiss_completions();
        editor.set_text("/help");
        editor.submit();
        editor.set_text("draft");
        editor.dismiss_completions();
        editor.up();
        assert_eq!(editor.text(), "/help");
        editor.down();
        assert_eq!(editor.text(), "draft");
    }

    #[test]
    fn mnemonic_commands_are_redacted_and_never_enter_history() {
        let command = format!("/session --mnemonic \"{MNEMONIC}\"");
        let parsed = parse_command(&command).expect("mnemonic command parses");
        let rendered = format!("{parsed:?}");
        assert!(!rendered.contains("abandon"), "mnemonic leaked: {rendered}");
        assert!(rendered.contains("<redacted>"));

        let masked = mask_mnemonic(&command).expect("mnemonic is masked");
        assert!(masked.starts_with("/session --mnemonic "));
        assert!(!masked.contains("abandon"));
        assert_eq!(masked.chars().count(), command.chars().count());
        let irregular = format!("  /session\t --mnemonic\t{MNEMONIC}");
        assert!(contains_mnemonic(&irregular));
        assert!(
            !mask_mnemonic(&irregular)
                .expect("irregular whitespace is still masked")
                .contains("abandon")
        );

        let mut editor = CommandEditor::default();
        editor.set_text(command.clone());
        let rendered = format!("{editor:?}");
        assert!(!rendered.contains("abandon"), "editor leaked: {rendered}");
        assert_eq!(editor.submit(), command);
        assert!(editor.history.is_empty());
    }

    #[test]
    fn editor_handles_unicode_at_character_boundaries() {
        let mut editor = CommandEditor::default();
        editor.set_text("/script café.ts");
        editor.left();
        editor.left();
        editor.left();
        editor.backspace();
        assert_eq!(editor.text(), "/script caf.ts");
    }

    #[test]
    fn approval_parser_is_trimmed_and_case_insensitive() {
        assert_eq!(parse_approval(" YES "), Some(true));
        assert_eq!(parse_approval("n"), Some(false));
        assert_eq!(parse_approval("sure"), None);
    }

    #[test]
    fn pair_command_advertises_image_paste_file_and_deeplink_inputs() {
        assert_eq!(
            completions_for_scope("/pair", &[], CommandScope::SigningHost),
            vec![Completion {
                value: "/pair".to_string(),
                description: "paste a pairing QR image or provide a file or URL",
            }]
        );
        assert!(HELP_TEXT.starts_with(
            "/pair                   paste or drop a pairing QR image\n\
             /pair <image-path>      read a pairing QR image file\n\
             /pair <url>             answer a Polkadot Mobile pairing URL"
        ));
    }

    #[test]
    fn script_completion_lists_matching_filesystem_paths() {
        let command = completions_for_scope("/script", &[], CommandScope::SigningHost);
        assert_eq!(command.len(), 1);
        assert_eq!(command[0].value, "/script");

        let prefix = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("signing_s");
        let matches = completions_for_scope(
            &format!("/script {}", prefix.display()),
            &[],
            CommandScope::SigningHost,
        );

        assert!(
            matches
                .iter()
                .any(|completion| completion.value.ends_with("signing_shell.rs"))
        );
    }

    #[test]
    fn session_completion_lists_existing_names() {
        let matches = completions_for_scope(
            "/session a",
            &["alice".to_string(), "bob".to_string()],
            CommandScope::SigningHost,
        );

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].value, "/session alice");
    }

    #[test]
    fn device_completion_offers_list_and_remove_operations() {
        assert_eq!(
            completions_for_scope("/devices --", &[], CommandScope::SigningHost),
            vec![
                Completion {
                    value: "/devices --list".to_string(),
                    description: "list paired devices",
                },
                Completion {
                    value: "/devices --remove".to_string(),
                    description: "remove one paired device",
                },
            ]
        );
        assert_eq!(
            completions_for_scope(
                &format!("/devices --remove {DEVICE_ID} --f"),
                &[],
                CommandScope::SigningHost
            ),
            vec![Completion {
                value: format!("/devices --remove {DEVICE_ID} --force"),
                description: "remove locally if notification fails",
            }]
        );
        assert!(completions_for_scope("/devices", &[], CommandScope::PairingHost).is_empty());
    }

    #[test]
    fn approval_completion_is_signing_host_only() {
        assert_eq!(
            completions_for_scope("/approval ", &[], CommandScope::SigningHost),
            vec![
                Completion {
                    value: "/approval manual".to_string(),
                    description: "prompt for every confirmation",
                },
                Completion {
                    value: "/approval automatic".to_string(),
                    description: "approve confirmations automatically",
                },
            ]
        );
        assert!(completions_for_scope("/appro", &[], CommandScope::PairingHost).is_empty());
    }

    #[test]
    fn session_completion_offers_clear_operations_and_existing_names() {
        let operations = completions_for_scope(
            "/session --c",
            &["alice".to_string(), "bob".to_string()],
            CommandScope::SigningHost,
        );
        assert!(
            operations
                .iter()
                .any(|completion| completion.value == "/session --clear")
        );
        assert!(
            operations
                .iter()
                .any(|completion| completion.value == "/session --clear-all")
        );
        let import = completions_for_scope(
            "/session --m",
            &["alice".to_string()],
            CommandScope::SigningHost,
        );
        assert_eq!(import.len(), 1);
        assert_eq!(import[0].value, "/session --mnemonic");

        let names = completions_for_scope(
            "/session --clear b",
            &["alice".to_string(), "bob".to_string()],
            CommandScope::SigningHost,
        );
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].value, "/session --clear bob");
    }

    #[test]
    fn pairing_host_completions_only_offer_shared_commands() {
        let matches = completions_for_scope("/", &[], CommandScope::PairingHost);
        let commands = matches
            .into_iter()
            .map(|completion| completion.value)
            .collect::<Vec<_>>();

        assert!(commands.contains(&"/script".to_string()));
        assert!(commands.contains(&"/login".to_string()));
        assert!(commands.contains(&"/logout".to_string()));
        assert!(commands.contains(&"/product".to_string()));
        assert!(commands.contains(&"/copy".to_string()));
        assert!(!commands.iter().any(|command| command.starts_with("/pair")));
        assert!(
            !commands
                .iter()
                .any(|command| command.starts_with("/session"))
        );
    }

    #[test]
    fn log_completion_offers_levels_after_command_for_both_hosts() {
        for scope in [CommandScope::SigningHost, CommandScope::PairingHost] {
            let root_matches = completions_for_scope("/log", &[], scope);
            assert_eq!(
                root_matches
                    .iter()
                    .filter(|completion| completion.value == "/log")
                    .count(),
                1
            );
            assert!(
                !root_matches
                    .iter()
                    .any(|completion| completion.value.starts_with("/log "))
            );

            let argument_matches = completions_for_scope("/log ", &[], scope);
            assert_eq!(
                argument_matches
                    .into_iter()
                    .map(|completion| completion.value)
                    .collect::<Vec<_>>(),
                [
                    "/log error",
                    "/log warn",
                    "/log info",
                    "/log debug",
                    "/log trace",
                ]
            );
        }
    }
}
