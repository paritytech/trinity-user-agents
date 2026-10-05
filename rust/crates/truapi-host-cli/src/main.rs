//! Headless TrUAPI hosts for local end-to-end testing.
//!
//! Two roles, one binary, pairing over the real People-chain statement store:
//! - `pairing-host`: a seedless host that presents a pairing deeplink and
//!   serves product frames over WebSocket (the surface a product/test driver
//!   talks to).
//! - `signing-host`: a wallet-local host that answers a pairing deeplink and
//!   auto-signs, replacing the external signing-bot in e2e.
//!
//! Plus the diagnostics and one-shot commands: `identity-check` for the dotNS
//! usernames of a mnemonic's accounts, `register-name` for a full-person
//! username, `alloc-check` for statement-store allowance, and `pgas-check` for
//! an Asset Hub PGAS allowance claim.

mod accounts;
mod attestation;
mod bootstrap;
mod bulletin_lookup;
mod chain;
mod chat;
mod contacts;
mod dotns_read;
mod frame_server;
mod network;
mod platform;
mod pocket;
mod product_config;
mod qr_scanner;
mod register_name;
mod script_project;
mod script_runner;
mod sessions;
mod signing_shell;
mod terminal_ui;
mod update;

use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::parser::ValueSource;
use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use futures::future::BoxFuture;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use truapi::host_logic::dotns_gateway::{
    MAX_BASE_LABEL_LEN, MIN_PERSON_LABEL_LEN, is_lite_label, is_registrable_full_label,
};
use truapi::platform::{
    ChatPlatform, HostInfo, PermissionStatusHost, PlatformInfo, ProductExecutionKind,
};
use truapi::statement_allowance as alloc;
use truapi::subscription::Spawner;
use truapi::{
    AnnouncedPairing, DebugSink, PairedSsoPeer, PairingHostConfig, PairingHostRuntime,
    PairingProposal, ResponderExit, SigningHostConfig, SigningHostRuntime, StatementRenewalTarget,
    WsDebugSink,
};

use crate::accounts::{ResolveSignerConfig, ResolvedSigner};
use crate::network::{Network, NetworkConfig};
use crate::platform::{ApprovalPolicy, CliPlatform, CliStoragePaths};
use crate::sessions::{
    CurrentSession, DEFAULT_SESSION_NAME, PairedHost, PairedHostMetadata, SessionCatalog,
    SessionClearTarget, SessionProfile,
};
use crate::signing_shell::{
    ApprovalCommand, DeviceCommand, HELP_TEXT, PAIRING_HELP_TEXT, PairCommand, ProductCommand,
    ScriptCommand, SessionCommand, ShellCommand, parse_command,
};
use crate::terminal_ui::{
    ActiveTerminalUi, ActivityState, DriveResult, PairingImageInput, SystemEvent, TerminalUi,
    UiHandle,
};

/// Default product served by the pairing host's frame endpoint. Product ids
/// must be a dotNS name (`.dot`, `.paseo` or `.testnet`) or a `localhost`
/// identifier (host-spec product id).
const DEFAULT_PRODUCT_ID: &str = "headless-playground.dot";
/// Deeplink scheme advertised by the pairing host.
const DEEPLINK_SCHEME: &str = "polkadotapp";
const LOG_LEVEL_FILE: &str = "log-level";
const STATE_VERSION: &str = "v2";

#[derive(Parser)]
#[command(
    name = "truapi-host",
    about = "Headless TrUAPI hosts for e2e testing",
    version = update::CURRENT_VERSION
)]
struct Cli {
    /// Log verbosity. Defaults to the saved `/log` level, then `info`.
    /// `RUST_LOG` takes precedence when set.
    #[arg(long, global = true, value_enum, env = "TRUAPI_HOST_LOG")]
    log_level: Option<LogLevel>,
    /// Stream every product frame to a wire debugger listening on this loopback
    /// `ws://` URL, e.g. `ws://127.0.0.1:9231`. The target must be loopback, and
    /// an unreachable debugger never fails a dispatch. Frames go out whole and
    /// the debugger decodes all of them, so a tapped signing host puts its
    /// payloads on that socket: point it at a debugger you run yourself. Only
    /// `pairing-host`, `dev` and `signing-host` read this.
    #[arg(long, global = true, env = "TRUAPI_DEBUGGER_URL", value_name = "URL")]
    debugger: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, derive_more::Display)]
#[display(rename_all = "lowercase")]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    const fn as_filter(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }

    fn scoped_filter(self) -> String {
        let level = self.as_filter();
        format!("warn,truapi={level},truapi_host={level}")
    }
}

impl FromStr for LogLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(format!(
                "invalid log level `{value}`; expected error, warn, info, debug, or trace"
            )),
        }
    }
}

fn resolve_log_level(selected: Option<LogLevel>, saved: Option<LogLevel>) -> LogLevel {
    selected.or(saved).unwrap_or(LogLevel::Info)
}

fn startup_filter(
    rust_log: Option<&str>,
    log_level: LogLevel,
) -> (tracing_subscriber::EnvFilter, String) {
    if let Some(value) = rust_log.map(str::trim).filter(|value| !value.is_empty())
        && let Ok(filter) = tracing_subscriber::EnvFilter::try_new(value)
    {
        return (filter, value.to_string());
    }
    (
        tracing_subscriber::EnvFilter::new(log_level.scoped_filter()),
        log_level.to_string(),
    )
}

fn load_log_level(base_path: &Path) -> Result<Option<LogLevel>> {
    let path = base_path.join(LOG_LEVEL_FILE);
    let value = match std::fs::read_to_string(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    value
        .trim()
        .parse()
        .map(Some)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("read saved log level from {}", path.display()))
}

fn store_log_level(base_path: &Path, level: LogLevel) -> Result<()> {
    let path = base_path.join(LOG_LEVEL_FILE);
    platform::atomic_write(&path, format!("{level}\n").as_bytes())
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("persist saved log level {}", path.display()))
}

#[derive(Clone)]
struct LogController {
    reload: Arc<dyn Fn(LogLevel) -> Result<(), String> + Send + Sync>,
    base_path: PathBuf,
}

impl LogController {
    fn set(&self, level: LogLevel) -> Result<()> {
        store_log_level(&self.base_path, level)?;
        (self.reload)(level).map_err(anyhow::Error::msg)
    }
}

#[derive(Subcommand)]
enum Command {
    /// Run a seedless pairing host for product scripts or interactive pairing.
    ///
    /// With `--script`, exits with the script's status. Without it, stays in an
    /// interactive terminal UI where scripts can be run repeatedly.
    PairingHost(PairingHostArgs),
    /// Run a product against a local signing host, with one command.
    ///
    /// Starts a host on loopback, waits for its signer, then runs the wrapped
    /// development command with the host already live. The product reaches it
    /// through a development-only `<script>` tag pointing at the bridge URL
    /// this prints.
    Dev(DevArgs),
    /// Run a wallet-local signing host for scripts or pairing deeplinks.
    ///
    /// Owns signer identity, auto-manages accounts when no mnemonic/account is
    /// specified, and can accept pairing deeplinks. With `--script`, exits with
    /// the script's status; otherwise stays interactive.
    SigningHost(SigningHostArgs),
    /// Probe the dotNS contracts on Asset Hub for a mnemonic's registered
    /// identity/username.
    IdentityCheck {
        /// BIP-39 mnemonic to probe.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to probe.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
    },
    /// Register a full-person username on dotNS via
    /// `DotnsGateway.register_name` on Asset Hub. Requires a recognized full
    /// person. That person's People ring must have propagated to Asset Hub.
    RegisterName {
        /// BIP-39 mnemonic of the recognized full person.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to use.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
        /// Base label to register (lowercase ASCII letters only).
        #[arg(long)]
        label: String,
        /// Dotted lite username to link (`name.NN`). Defaults to the
        /// account's own lite username.
        #[arg(long, conflicts_with = "chat_key")]
        link_lite: Option<String>,
        /// 65-byte ECDH chat key (hex). Use it for a standalone registration
        /// with no lite-username link.
        #[arg(long)]
        chat_key: Option<String>,
    },
    /// Check (and optionally submit) a statement-store allowance registration
    /// against the real People chain: ring membership, the chosen slot, and
    /// (with `--submit`) the `set_statement_store_account` extrinsic.
    AllocCheck {
        /// BIP-39 mnemonic proving LitePeople ring membership.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to use for People-chain RPC.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
        /// Target account (hex, 32 bytes) to grant allowance to. Defaults to
        /// all-zero (read-only slot scan only).
        #[arg(long)]
        target: Option<String>,
        /// How many rings back from the current index to scan for our member.
        #[arg(long, default_value_t = 8)]
        lookback: u32,
        /// Submit the extrinsic instead of only checking membership + slot.
        #[arg(long)]
        submit: bool,
    },
    /// Diagnose (or `--submit`) an Asset Hub PGAS allowance claim: ring
    /// membership on People, revision propagation to Asset Hub, the day's free
    /// slot, and the `Pgas.claim_pgas` extrinsic.
    PgasCheck {
        /// BIP-39 mnemonic proving LitePeople ring membership.
        #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
        mnemonic: String,
        /// Network preset to use for People and Asset Hub RPC.
        #[arg(long, value_enum, default_value = "paseo-next-v2")]
        network: Network,
        /// Account (hex, 32 bytes) the claim credits. Required with `--submit`.
        #[arg(long)]
        target: Option<String>,
        /// How many rings back from the current index to scan for our member.
        #[arg(long, default_value_t = 8)]
        lookback: u32,
        /// Submit the claim instead of only reporting what it would do.
        #[arg(long)]
        submit: bool,
    },
    /// Install the current stable release over this one.
    ///
    /// Only works for a binary the installer put in place; a `cargo install`
    /// copy or a source build is reported and left alone.
    Update,
}

/// Execution kind the CLI serves a product as.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum ExecutionKind {
    /// Ordinary full-page product. Chat requests answer `Denied`, as they do
    /// on any host that does not serve chat.
    #[default]
    App,
    /// Headless executable served by the CLI's in-memory chat host, plus its
    /// in-memory Pocket host when `TRUAPI_POCKET_CARDS` names a card set.
    Worker,
}

impl ExecutionKind {
    fn context(self) -> ProductExecutionKind {
        match self {
            Self::App => ProductExecutionKind::App,
            Self::Worker => ProductExecutionKind::Worker,
        }
    }

    /// The chat host to install, if this kind serves chat at all.
    fn chat_host(self) -> Option<Arc<chat::CliChatHost>> {
        matches!(self, Self::Worker).then(chat::CliChatHost::from_env)
    }

    /// The Pocket host to install, if this kind serves Pocket and a card set
    /// was configured.
    fn pocket_host(self) -> Option<Arc<pocket::CliPocketHost>> {
        matches!(self, Self::Worker)
            .then(pocket::CliPocketHost::from_env)
            .flatten()
    }
}

#[derive(Args)]
struct PairingHostArgs {
    /// Execution kind the served product runs as. `worker` installs the CLI's
    /// in-memory chat host; `app` leaves Chat unserved. `worker` also installs
    /// the Pocket host when `TRUAPI_POCKET_CARDS` is set.
    #[arg(long = "execution-kind", value_enum, default_value = "app")]
    execution_kind: ExecutionKind,
    /// Product script to run (JS/TS). If omitted, start the terminal UI.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Product id the host serves; scopes storage and product accounts.
    #[arg(long = "product-id", default_value = DEFAULT_PRODUCT_ID)]
    product_id: String,
    /// TCP address to serve product WebSocket frames on. When omitted, use a
    /// private per-process Unix-domain socket.
    #[arg(long)]
    frame_listen: Option<SocketAddr>,
    /// Base directory; CLI-managed state lives in its v2 subdirectory.
    #[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
    base_path: Option<PathBuf>,
    /// Network preset that supplies all RPC/backend/genesis config.
    #[arg(long, value_enum, default_value = "paseo-next-v2")]
    network: Network,
    /// Approve every confirmation without prompting on the CLI.
    #[arg(long)]
    auto_accept: bool,
}

/// Default loopback port for the frame socket and the bridge script.
///
/// Products name it in a development `<script>` tag, so it is part of their
/// source and stays fixed unless both sides change together.
const DEFAULT_DEV_PORT: u16 = 9955;

#[derive(Args)]
struct DevArgs {
    /// Port the wrapped development server listens on. Names the product id.
    #[arg(long = "app-port", default_value_t = 3000)]
    app_port: u16,
    /// Loopback port serving product frames and the bridge script.
    #[arg(long, default_value_t = DEFAULT_DEV_PORT)]
    port: u16,
    /// Product id the host serves. Defaults to the development server's own
    /// origin, which is what a product derives for itself locally.
    #[arg(long = "product-id")]
    product_id: Option<String>,
    /// Network preset that supplies all RPC/backend/genesis config.
    #[arg(long, value_enum, default_value = "paseo-next-v2")]
    network: Network,
    /// Persistent signing-host session to restore or create.
    #[arg(long)]
    session: Option<String>,
    /// BIP-39 mnemonic for the wallet root, to sign as an existing identity
    /// instead of the auto-managed one. Confirmations are auto-approved here,
    /// so the connected product can sign anything: testnet keys only.
    #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
    mnemonic: Option<String>,
    /// Base directory; CLI-managed state lives in its v2 subdirectory.
    #[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
    base_path: Option<PathBuf>,
    /// Local product config declaring `trustedProducts`, as the publisher will
    /// read it. Repeat to serve a grant between two products. The grants are
    /// applied for this run only and never reach a chain.
    #[arg(long = "product-config")]
    product_config: Vec<PathBuf>,
    /// Development command to run once the host is ready, after `--`.
    #[arg(last = true)]
    command: Vec<String>,
}

#[derive(Args, Default)]
struct SigningHostArgs {
    /// Execution kind the served product runs as. `worker` installs the CLI's
    /// in-memory chat host; `app` leaves Chat unserved. `worker` also installs
    /// the Pocket host when `TRUAPI_POCKET_CARDS` is set.
    #[arg(long = "execution-kind", value_enum, default_value = "app")]
    execution_kind: ExecutionKind,
    /// Product script to run (JS/TS). If omitted, start an interactive shell.
    #[arg(long)]
    script: Option<PathBuf>,
    /// Product id used by scripts and product-scoped operations.
    #[arg(long = "product-id", default_value = DEFAULT_PRODUCT_ID)]
    product_id: String,
    /// Pairing deeplink to add. Managed interactive and serve sessions also
    /// restore responders for every previously paired host.
    #[arg(long)]
    deeplink: Option<String>,
    /// BIP-39 mnemonic for the wallet root. If omitted, the
    /// `HOST_CLI_SIGNER_MNEMONIC` env var is used when set. Any mnemonic
    /// bypasses account auto-management.
    #[arg(long, env = "HOST_CLI_SIGNER_MNEMONIC")]
    mnemonic: Option<String>,
    /// Named stored account to use. Omit this and `--mnemonic` to auto-select
    /// or create a usable account.
    #[arg(long)]
    account: Option<String>,
    /// Select the newest saved session for a username base, or the exact numbered username.
    /// Interactive startup provisions a new base when needed; exec leaves that to its command.
    #[arg(long)]
    session: Option<String>,
    /// Full-person base name a newly-created auto account reserves on dotNS
    /// alongside its lite username, to claim later as a full person.
    #[arg(long = "reserved-username")]
    reserved_username: Option<String>,
    /// Base directory; CLI-managed state lives in its v2 subdirectory.
    #[arg(long = "base-path", env = "TRUAPI_HOST_BASE_PATH")]
    base_path: Option<PathBuf>,
    /// Network preset that supplies all RPC/backend/genesis config.
    #[arg(long, value_enum, default_value = "paseo-next-v2")]
    network: Network,
    /// TCP address to serve product WebSocket frames on. When omitted, use a
    /// private per-process Unix-domain socket.
    #[arg(long)]
    frame_listen: Option<SocketAddr>,
    /// Approve every confirmation without prompting on the CLI.
    #[arg(long)]
    auto_accept: bool,
    /// Serve product frames without a terminal UI and stay up until stopped.
    /// Needs no TTY, so a dev server can supervise this process: the frame
    /// endpoint and every lifecycle event are logged one line at a time, and
    /// the signer is ready once "Signing host ready" is printed. Pair it with
    /// `--auto-accept`, because a process with no terminal cannot prompt.
    #[arg(long)]
    serve: bool,
    /// Local product config declaring `trustedProducts`, as the publisher will
    /// read it. Repeat to serve a grant between two products.
    #[arg(long = "product-config")]
    product_config: Vec<PathBuf>,
    /// Execute one slash command without starting the terminal UI.
    #[command(subcommand)]
    action: Option<SigningHostAction>,
}

#[derive(Subcommand)]
enum SigningHostAction {
    /// Execute one slash command and exit.
    Exec {
        /// Slash command to execute, such as `/session`.
        command: String,
    },
}

fn command_base_path(command: &Command) -> PathBuf {
    let base_path = match command {
        Command::PairingHost(args) => args.base_path.clone(),
        Command::Dev(args) => args.base_path.clone(),
        Command::SigningHost(args) => args.base_path.clone(),
        _ => None,
    };
    state_base_path(base_path)
}

fn state_base_path(base_path: Option<PathBuf>) -> PathBuf {
    base_path
        .unwrap_or_else(default_base_path)
        .join(STATE_VERSION)
}

fn script_project_directory(base_path: Option<PathBuf>) -> PathBuf {
    base_path.unwrap_or_else(default_base_path).join("scripts")
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install a rustls crypto provider so `wss://` chain connections work;
    // rustls 0.23 panics without a process-level default provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let matches = Cli::command().get_matches();
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    let debugger = cli.debugger.take().map(|url| DebuggerSwitch {
        url,
        source: debugger_url_source(&matches),
    });
    let base_path = command_base_path(&cli.command);
    let (saved_log_level, saved_log_level_error) = match load_log_level(&base_path) {
        Ok(level) => (level, None),
        Err(error) => (None, Some(error)),
    };
    let log_level = resolve_log_level(cli.log_level, saved_log_level);
    let rust_log = std::env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV).ok();
    let (filter, log_filter) = startup_filter(rust_log.as_deref(), log_level);
    let (filter, reload) = tracing_subscriber::reload::Layer::new(filter);
    let log_controller = LogController {
        reload: Arc::new(move |level| {
            reload
                .reload(tracing_subscriber::EnvFilter::new(level.scoped_filter()))
                .map_err(|error| error.to_string())
        }),
        base_path,
    };
    let log_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_level(false)
        .with_writer(terminal_ui::LogWriter::default)
        .with_filter(filter)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            log_target_is_visible(metadata.target())
        }));
    tracing_subscriber::registry()
        .with(terminal_ui::SsoTranscriptLayer)
        .with(log_layer)
        .init();

    if let Some(error) = saved_log_level_error {
        tracing::warn!(%error, "could not restore saved log level; using fallback");
    }

    // A background check runs alongside the command rather than delaying it, and
    // reports through `tracing`, which the terminal UI renders in its transcript
    // so it cannot corrupt the full-screen display. The command then waits for
    // it at exit, so even a short one completes the download it started.
    let check = (!matches!(cli.command, Command::Update)).then(|| {
        update::report_install();
        tokio::spawn(update::run_background_check())
    });

    let outcome = dispatch(cli.command, log_filter, log_controller, debugger).await;

    if let Some(check) = check {
        update::finish_background_check(check).await;
    }
    outcome
}

/// Run the requested command.
///
/// Separate from `main` so that the `?` several arms use returns here, leaving
/// `main` free to always wait for the update check it started.
async fn dispatch(
    command: Command,
    log_filter: String,
    log_controller: LogController,
    debugger: Option<DebuggerSwitch>,
) -> Result<()> {
    // The switch is global but resolved per command, by the three arms that build
    // a frame server and still before they bind a port. `update`,
    // `identity-check`, `register-name` and `alloc-check` emit no frames, so they
    // open no sink and cannot be failed by a URL they would never dial - which
    // matters most when `TRUAPI_DEBUGGER_URL` is exported and every invocation
    // carries the switch.
    match command {
        Command::Update => update::run_update_command().await,
        Command::PairingHost(args) => {
            run_pairing_host(args, log_filter, log_controller, debugger).await
        }
        Command::Dev(args) => run_dev(args, log_filter, log_controller, debugger).await,
        Command::SigningHost(args) => {
            run_signing_host(args, log_filter, log_controller, None, debugger).await
        }
        Command::IdentityCheck { mnemonic, network } => {
            let entropy = bip39::Mnemonic::parse(mnemonic.trim())
                .context("invalid BIP-39 mnemonic")?
                .to_entropy();
            attestation::check_identity(network.config(), &entropy).await
        }
        Command::RegisterName {
            mnemonic,
            network,
            label,
            link_lite,
            chat_key,
        } => {
            let entropy = bip39::Mnemonic::parse(mnemonic.trim())
                .context("invalid BIP-39 mnemonic")?
                .to_entropy();
            let chat_key = chat_key
                .map(|value| -> Result<[u8; 65]> {
                    let bytes = hex::decode(value.strip_prefix("0x").unwrap_or(&value))
                        .context("chat key is not valid hex")?;
                    bytes.try_into().map_err(|bytes: Vec<u8>| {
                        anyhow::anyhow!("chat key must be 65 bytes, got {}", bytes.len())
                    })
                })
                .transpose()?;
            register_name::register_name(&register_name::RegisterNameConfig {
                network: network.config(),
                entropy,
                label,
                link_lite,
                chat_key,
            })
            .await
        }
        Command::AllocCheck {
            mnemonic,
            network,
            target,
            lookback,
            submit,
        } => run_alloc_check(mnemonic, network.config(), target, lookback, submit).await,
        Command::PgasCheck {
            mnemonic,
            network,
            target,
            lookback,
            submit,
        } => run_pgas_check(mnemonic, network.config(), target, lookback, submit).await,
    }
}

/// Diagnose an Asset Hub PGAS claim, and optionally submit it.
///
/// Connects to both chains directly rather than through the host's
/// `ChainProvider`, because a diagnostic should not depend on a host being wired
/// up. The provider would route Asset Hub correctly now that the preset serves it
/// as a role, so this is a choice about the subcommand rather than a workaround.
async fn run_pgas_check(
    mnemonic: String,
    network: crate::network::NetworkConfig,
    target: Option<String>,
    lookback: u32,
    submit: bool,
) -> Result<()> {
    use std::sync::Arc;

    use truapi::statement_allowance::pgas;

    let entropy = bip39::Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy();
    let candidates = accounts::collection_candidates(&entropy, network.network_suffix);

    if submit && target.is_none() {
        bail!("--target is required with --submit; a claim has to credit an account");
    }
    let target = match target {
        Some(hex_str) => {
            let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(&hex_str))
                .context("invalid --target hex")?;
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("--target must be 32 bytes"))?
        }
        None => [0u8; 32],
    };

    let people_rpc = alloc::rpc::RpcClient::connect(network.people_ws)
        .await
        .map_err(anyhow::Error::msg)?;
    let people_metadata = alloc::fetch_metadata(&people_rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let asset_hub_rpc = alloc::rpc::RpcClient::connect(network.asset_hub_ws)
        .await
        .map_err(anyhow::Error::msg)?;
    let asset_hub_metadata = alloc::fetch_metadata(&asset_hub_rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let asset_hub_state = alloc::fetch_chain_state(&asset_hub_rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let network_suffix = alloc::slot::read_network_suffix(&asset_hub_rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    println!(
        "asset hub: metadata V{} specVersion={} txVersion={} genesis=0x{}",
        asset_hub_metadata.metadata_version(),
        asset_hub_state.spec_version,
        asset_hub_state.transaction_version,
        hex::encode(asset_hub_state.genesis_hash),
    );

    for candidate in &candidates {
        println!(
            "{} member=0x{}",
            candidate.collection,
            hex::encode(alloc::proof::member_key(candidate.entropy).await?)
        );
    }
    let memberships =
        alloc::find_including_rings(&people_rpc, &people_metadata, &candidates, lookback)
            .await
            .map_err(anyhow::Error::msg)?;
    for membership in &memberships {
        println!(
            "member INCLUDED in {} ring_index={} exponent={} members={}",
            membership.collection(),
            membership.ring.ring_index,
            membership.ring.exponent,
            membership.ring.members.len()
        );
    }
    // The widest membership the signer actually holds; a claim needs exactly one.
    let Some(membership) = memberships.first() else {
        bail!("member is not in the last {lookback} rings of any collection (onboarding pending)");
    };
    let ring = &membership.ring;

    let revision = alloc::ring::read_ring_revision(
        &people_rpc,
        &people_metadata,
        ring.collection,
        ring.ring_index,
        &ring.block_hash,
    )
    .await
    .map_err(anyhow::Error::msg)?;
    println!("people ring revision={revision}");
    match pgas::await_ring_revision(
        &asset_hub_rpc,
        &asset_hub_metadata,
        ring.collection,
        ring.ring_index,
        revision,
    )
    .await
    {
        Ok(()) => println!("asset hub has imported revision {revision}"),
        Err(err) => bail!("asset hub cannot authorize this ring: {err}"),
    }

    let day = alloc::slot::current_period(
        alloc::slot::read_chain_now_seconds(&asset_hub_rpc)
            .await
            .map_err(anyhow::Error::msg)?,
    );
    let max = alloc::slot::max_pgas_claims(&asset_hub_metadata, ring.collection)
        .map_err(anyhow::Error::msg)?;
    println!(
        "day={day} max_claims_per_day={max} target=0x{}",
        hex::encode(target)
    );
    match alloc::slot::scan_pgas_slot_excluding(
        &asset_hub_rpc,
        &asset_hub_metadata,
        ring.collection,
        membership.entropy,
        &network_suffix,
        day,
        &[],
    )
    .await
    {
        Ok(slot_index) => println!("slot scan: free slot_index={slot_index}"),
        Err(err) => println!("slot scan: {err}"),
    }

    if !submit {
        return Ok(());
    }

    let asset_hub = alloc::ChainContext {
        metadata: Arc::new(asset_hub_metadata),
        state: asset_hub_state,
    };
    let outcome = pgas::claim_pgas(pgas::PgasClaim {
        asset_hub_rpc: &asset_hub_rpc,
        asset_hub: &asset_hub,
        people_rpc: &people_rpc,
        people_metadata: &people_metadata,
        entropy: membership.entropy,
        network_suffix: &network_suffix,
        target: &target,
        ring: &membership.ring,
    })
    .await
    .map_err(|err| anyhow::anyhow!("claim failed: {err}"))?;
    println!(
        "CLAIMED day={} slot_index={} ring_index={} block={}",
        outcome.day, outcome.slot_index, outcome.ring_index, outcome.block_hash
    );
    Ok(())
}

fn log_target_is_visible(target: &str) -> bool {
    target != terminal_ui::SSO_TRANSCRIPT_TARGET
        && target != "rustls"
        && !target.starts_with("rustls::")
        && target != "tungstenite::protocol"
        && !target.starts_with("tungstenite::protocol::")
}

/// Check statement-store allowance for a mnemonic: ring membership, the chosen
/// slot, and (with `submit`) the `set_statement_store_account` extrinsic.
async fn run_alloc_check(
    mnemonic: String,
    network: NetworkConfig,
    target: Option<String>,
    lookback: u32,
    submit: bool,
) -> Result<()> {
    let entropy = bip39::Mnemonic::parse(mnemonic.trim())
        .context("invalid BIP-39 mnemonic")?
        .to_entropy();
    let candidates = accounts::collection_candidates(&entropy, network.network_suffix);

    if submit && target.is_none() {
        bail!("--target is required with --submit; the all-zero default is read-only");
    }

    let target = match target {
        Some(hex_str) => {
            let bytes = hex::decode(hex_str.strip_prefix("0x").unwrap_or(&hex_str))
                .context("invalid --target hex")?;
            <[u8; 32]>::try_from(bytes.as_slice())
                .map_err(|_| anyhow::anyhow!("--target must be 32 bytes"))?
        }
        None => [0u8; 32],
    };

    let rpc = alloc::rpc::RpcClient::connect(network.people_ws)
        .await
        .map_err(anyhow::Error::msg)?;
    let metadata = alloc::fetch_metadata(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let chain_state = alloc::fetch_chain_state(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    let network_suffix = alloc::slot::read_network_suffix(&rpc)
        .await
        .map_err(anyhow::Error::msg)?;
    println!(
        "chain: specVersion={} txVersion={} genesis=0x{}",
        chain_state.spec_version,
        chain_state.transaction_version,
        hex::encode(chain_state.genesis_hash),
    );

    for candidate in &candidates {
        println!(
            "{} member=0x{} current_ring_index={}",
            candidate.collection,
            hex::encode(alloc::proof::member_key(candidate.entropy).await?),
            alloc::ring::read_current_ring_index(&rpc, candidate.collection)
                .await
                .map_err(anyhow::Error::msg)?,
        );
    }
    let memberships = alloc::find_including_rings(&rpc, &metadata, &candidates, lookback)
        .await
        .map_err(anyhow::Error::msg)?;
    if memberships.is_empty() {
        println!("member NOT in the last {lookback} rings of any collection (onboarding pending)");
    }
    for membership in &memberships {
        println!(
            "member INCLUDED in {} ring_index={} exponent={} included_members={}",
            membership.collection(),
            membership.ring.ring_index,
            membership.ring.exponent,
            membership.ring.members.len(),
        );
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock before UNIX epoch")?
        .as_secs();
    let period = alloc::slot::current_period(now);
    println!("period={period} target=0x{}", hex::encode(target));

    for candidate in &candidates {
        if !candidate.collection.is_supported(&metadata) {
            println!("{}: not offered by this chain", candidate.collection);
            continue;
        }
        print!("{}: ", candidate.collection);
        report_slot_scan(
            &rpc,
            &metadata,
            *candidate,
            &network_suffix,
            period,
            &target,
            now,
        )
        .await?;
    }

    if submit {
        if memberships.is_empty() {
            bail!("cannot submit: member not in any ring");
        }
        let scans = alloc::scan_collections(
            &rpc,
            &metadata,
            &candidates,
            &network_suffix,
            period,
            &target,
            true,
        )
        .await
        .map_err(anyhow::Error::msg)?;
        match alloc::register_statement_account_pooled(
            &rpc,
            &metadata,
            &chain_state,
            &scans,
            &memberships,
            alloc::PooledRegistrationParams {
                target: &target,
                period,
                network_suffix: &network_suffix,
                reuse_existing: true,
                // A diagnostic that submits behaves as it did before pooling,
                // where a full table was replaced rather than reported.
                allow_eviction: true,
                protected: &[],
            },
        )
        .await
        {
            Ok(alloc::RegistrationOutcome::Registered {
                block_hash,
                seq,
                ring_index,
                collection,
            }) => println!(
                "REGISTERED in {collection} seq={seq} ring_index={ring_index} block={block_hash}"
            ),
            Ok(alloc::RegistrationOutcome::AlreadyAllocated { seq, collection }) => {
                println!("already allocated in {collection} at seq={seq}")
            }
            Err(err) => bail!("registration failed: {err}"),
        }
    }

    Ok(())
}

/// Print one collection's slot table for `period`: the free or reusable slot, or
/// the full table with each occupant's age against the chain clock.
async fn report_slot_scan(
    rpc: &alloc::rpc::RpcClient,
    metadata: &alloc::extension::Metadata,
    candidate: alloc::CollectionCandidate,
    network_suffix: &[u8],
    period: u32,
    target: &[u8; 32],
    now: u64,
) -> Result<()> {
    match alloc::slot::scan_slot_excluding(
        rpc,
        metadata,
        alloc::slot::SlotScan {
            collection: candidate.collection,
            entropy: candidate.entropy,
            network_suffix,
            period,
            target,
            excluded: &[],
            reuse_existing: true,
        },
    )
    .await
    {
        Ok(alloc::slot::SlotSelection::Free(seq)) => println!("slot scan: free seq={seq}"),
        Ok(alloc::slot::SlotSelection::FreeSlotsExcluded) => {
            println!("slot scan: free slots exist but are awaiting earlier submissions");
        }
        Ok(alloc::slot::SlotSelection::Full { max, occupied }) => {
            println!("slot scan: all {max} slots taken, none reusable");
            let cooldown = alloc::slot::replacement_cooldown(rpc, metadata).await?;
            // The runtime judges ages against its own clock, which trails ours.
            let chain_now = alloc::slot::read_chain_now_seconds(rpc).await?;
            println!(
                "  chain clock={chain_now} (host clock is {}s ahead)",
                now.saturating_sub(chain_now)
            );
            for slot in &occupied {
                let age = chain_now.saturating_sub(slot.since);
                let state = if age >= cooldown {
                    "replaceable"
                } else {
                    "in cooldown"
                };
                println!(
                    "  seq={} since={} age={age}s {state} account=0x{}",
                    slot.seq,
                    slot.since,
                    hex::encode(slot.account_id)
                );
            }
            match alloc::slot::replaceable_slot(&occupied, target, chain_now, cooldown, &[]) {
                Some(seq) => println!("would replace seq={seq} (oldest replaceable)"),
                None => println!("nothing replaceable: cooldown={cooldown}s"),
            }
        }
        Ok(alloc::slot::SlotSelection::AlreadyAllocated(seq)) => {
            println!("slot scan: target already allocated at seq={seq}")
        }
        Err(err) => println!("slot scan: {err}"),
    }

    Ok(())
}

/// Map the `--auto-accept` flag to an approval policy: auto-accept, or prompt
/// each confirmation on the CLI.
fn approval_policy(auto_accept: bool) -> ApprovalPolicy {
    if auto_accept {
        ApprovalPolicy::AutoAccept
    } else {
        ApprovalPolicy::Prompt
    }
}

/// Spawner that runs runtime futures on the tokio runtime, so their WebSocket
/// connects and timers have a reactor.
///
/// The core spawns teardown work from `Drop`, which can run after the runtime
/// has shut down; `tokio::spawn` panics there, so the handle is looked up
/// rather than assumed.
fn tokio_spawner() -> Spawner {
    Arc::new(
        |fut: BoxFuture<'static, ()>| match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(fut);
            }
            Err(error) => {
                tracing::warn!(%error, "dropping a runtime future: no tokio runtime is running");
            }
        },
    )
}

fn host_info(name: &str) -> HostInfo {
    HostInfo {
        name: name.to_string(),
        icon: None,
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        platform: truapi::latest::HostPlatform::Cli,
    }
}

fn platform_info() -> PlatformInfo {
    PlatformInfo {
        kind: Some("cli".to_string()),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
    }
}

/// A `--debugger` switch as clap resolved it: the URL in play, and the spelling
/// clap took it from.
struct DebuggerSwitch {
    url: String,
    source: &'static str,
}

/// A resolved `--debugger` switch with its sink, carried so the dial can be
/// announced after the terminal UI exists.
struct DebuggerDial {
    sink: Arc<dyn DebugSink>,
    switch: DebuggerSwitch,
}

/// Name the switch clap read `--debugger` from.
///
/// §9 requires a host that dials to name the source it read, so a developer
/// looking at a host that streams knows which spelling to clear. clap folds the
/// flag and the variable into one value, and `value_source` is the only thing
/// that still knows which of the two it took.
fn debugger_url_source(matches: &ArgMatches) -> &'static str {
    match matches.value_source("debugger") {
        Some(ValueSource::EnvVariable) => "TRUAPI_DEBUGGER_URL",
        _ => "--debugger",
    }
}

/// Open a wire-debugger sink for `switch`.
///
/// A URL that is not a loopback `ws://` target aborts startup rather than
/// warning. The caller explicitly asked for a debugger, and a host that runs on
/// without one is indistinguishable, from the debugger's side, from a host that
/// is simply idle. Success does not mean the debugger is listening: the sink
/// dials lazily and reconnects, so it can be started either side of the host.
fn connect_debugger(switch: DebuggerSwitch) -> Result<DebuggerDial> {
    let DebuggerSwitch { url, source } = &switch;
    let sink = WsDebugSink::connect(url).with_context(|| {
        format!("{source} {url} must be a ws:// URL on 127.0.0.1, localhost, or [::1]")
    })?;
    Ok(DebuggerDial { sink, switch })
}

/// Say once whether this host streams frames, and to where.
///
/// §9 requires both arms. Silence when no dial is set is the failure the rule
/// exists for: the debugger's own viewer holds a socket, so its socket count
/// moves whether or not a host connected, and an empty board is indistinguishable
/// from a host nobody switched on. Announcing the dial is what makes an ungated
/// switch acceptable on a released binary (§7) - a host streaming frames is never
/// quiet about it, so a stale exported variable cannot tap a session unnoticed.
fn report_debugger(dial: Option<&DebuggerDial>) {
    match dial {
        Some(dial) => terminal_ui::output_event(SystemEvent::DebuggerDialling {
            url: dial.switch.url.clone(),
            source: dial.switch.source.to_string(),
        }),
        None => terminal_ui::output_event(SystemEvent::DebuggerOff),
    }
}

/// Wrap `factory` so every product frame it serves also reaches `sink`,
/// returning it untouched when no debugger was requested.
fn tap_for_debugger(
    factory: Arc<dyn frame_server::ProductRuntimeFactory>,
    sink: Option<Arc<dyn DebugSink>>,
) -> Arc<dyn frame_server::ProductRuntimeFactory> {
    match sink {
        Some(sink) => frame_server::DebugTappedRuntime::new(factory, sink),
        None => factory,
    }
}

#[cfg(test)]
mod debugger_tap_tests {
    use super::*;
    use truapi::{DebugEvent, FrameSink, ProductContext, ProductRuntime};

    struct SilentSink;

    impl DebugSink for SilentSink {
        fn emit(&self, _event: DebugEvent) {}
    }

    struct UnusedFactory;

    impl frame_server::ProductRuntimeFactory for UnusedFactory {
        fn product_runtime(
            &self,
            _product: ProductContext,
            _sink: Arc<dyn FrameSink>,
        ) -> ProductRuntime {
            panic!("this test observes the wrapping decision only")
        }
    }

    /// What `DebugTappedRuntime` does once installed is covered next to it. This
    /// covers whether it is installed at all: a `tap_for_debugger` that returned
    /// `factory` in both arms leaves a host that accepts `--debugger`, announces
    /// the dial, and streams nothing.
    #[test]
    fn the_tap_wraps_the_factory_exactly_when_a_sink_was_opened() {
        let factory: Arc<dyn frame_server::ProductRuntimeFactory> = Arc::new(UnusedFactory);
        let tapped = tap_for_debugger(factory.clone(), Some(Arc::new(SilentSink)));
        let untapped = tap_for_debugger(factory.clone(), None);

        assert_eq!(
            (
                Arc::ptr_eq(&factory, &tapped),
                Arc::ptr_eq(&factory, &untapped)
            ),
            (false, true)
        );
    }
}

async fn run_pairing_host(
    args: PairingHostArgs,
    initial_log_filter: String,
    log_controller: LogController,
    debugger: Option<DebuggerSwitch>,
) -> Result<()> {
    let script_projects = script_project_directory(args.base_path.clone());
    let interactive = args.script.is_none();
    if interactive && !terminal_ui::is_interactive_terminal() {
        invalid_invocation(
            "interactive pairing-host requires a TTY; use pairing-host --script <path>",
        );
    }
    let network = args.network.config();
    let base_path = state_base_path(args.base_path);
    let product =
        frame_server::ProductSelection::new(args.product_id, args.execution_kind.context())?;
    let product_id = product.current();
    let storage_paths = CliStoragePaths::pairing(base_path.join(network.id));
    let (terminal_ui, ui_handle) = if interactive {
        let (ui, handle) =
            TerminalUi::new_pairing(network.id, product_id.clone(), initial_log_filter);
        (Some(ui.enter()?), Some(handle))
    } else {
        (None, None)
    };
    let platform = CliPlatform::new(
        network,
        Some(storage_paths),
        approval_policy(args.auto_accept),
        ui_handle,
    );
    // SSO runs over the real People chain. Usernames resolve from the dotNS
    // contracts on Asset Hub.
    let config = PairingHostConfig::new(
        host_info("Headless Pairing Host"),
        platform_info(),
        network.people_genesis,
        network.bulletin_genesis,
        network.asset_hub_genesis,
        DEEPLINK_SCHEME.to_string(),
    )
    .context("invalid pairing host config")?;
    let storage_platform = platform.clone();
    let chat_host = args.execution_kind.chat_host();
    let pocket_host = args.execution_kind.pocket_host();
    let status_host = platform.clone() as Arc<dyn PermissionStatusHost>;
    let pairing_runtime = Arc::new(PairingHostRuntime::with_chat_platform(
        platform,
        config,
        tokio_spawner(),
        chat_host.map(|chat| chat as Arc<dyn ChatPlatform>),
    ));
    pairing_runtime.set_permission_status_host(status_host);
    if let Some(pocket) = pocket_host {
        pairing_runtime.set_pocket_platform(pocket);
    }
    pairing_runtime.set_contacts_platform(contacts::CliContactsHost::from_env(
        storage_platform.clone(),
    ));

    // Resolved before the port is bound, so a bad URL still fails on the argument
    // rather than half-way through startup - but reported below, once the UI
    // exists. A `tracing` line here goes to a stderr that the alternate screen
    // covers for the rest of the run, so in interactive mode nobody ever sees it.
    let debugger = debugger.map(connect_debugger).transpose()?;
    let frame_server = frame_server::bind(args.frame_listen).await?;
    let frame_url = frame_server.endpoint().to_string();
    terminal_ui::output_event(SystemEvent::FramesListening {
        url: frame_url.clone(),
    });
    report_debugger(debugger.as_ref());
    let runtime_for_frames = tap_for_debugger(pairing_runtime.clone(), debugger.map(|d| d.sink));

    if let Some(script) = args.script {
        let script_product_id = product_id.clone();
        let script_frame_url = frame_url.clone();
        let status = with_frame_server(runtime_for_frames, product, frame_server, async move {
            script_runner::run(
                &script_frame_url,
                &script_product_id,
                &script,
                script_runner::ScriptHostRole::PairingHost,
            )
            .await
        })
        .await?;
        let code = status.code().unwrap_or(1);
        terminal_ui::output_event(SystemEvent::ScriptExit { code });
        std::process::exit(code);
    }

    let terminal_ui = terminal_ui.context("interactive terminal was not initialized")?;
    with_frame_server(
        runtime_for_frames,
        product.clone(),
        frame_server,
        async move {
            pairing_interactive_loop(
                frame_url,
                product,
                pairing_runtime,
                storage_platform,
                script_projects,
                terminal_ui,
                log_controller,
            )
            .await
        },
    )
    .await
}

async fn run_signing_host(
    args: SigningHostArgs,
    initial_log_filter: String,
    log_controller: LogController,
    dev_command: Option<Vec<String>>,
    debugger: Option<DebuggerSwitch>,
) -> Result<()> {
    if let Err(error) = validate_signing_args(&args) {
        invalid_invocation(error);
    }
    let exec_input = args
        .action
        .as_ref()
        .map(|SigningHostAction::Exec { command }| command.clone());
    let interactive = args.script.is_none() && exec_input.is_none() && !args.serve;
    if interactive && !terminal_ui::is_interactive_terminal() {
        invalid_invocation(
            "interactive signing-host requires a TTY; use --serve to run headless, or `signing-host exec '/script path.ts'`, or --script",
        );
    }
    let exec_command = exec_input
        .as_deref()
        .map(|input| parse_command(input).unwrap_or_else(|error| invalid_invocation(error)));
    let product = frame_server::ProductSelection::new(
        args.product_id.clone(),
        args.execution_kind.context(),
    )?;
    let product_id = product.current();
    let network = args.network.config();
    let base_path = state_base_path(args.base_path.clone());
    let session_catalog = SessionCatalog::new(base_path.clone(), network.id)?;
    let initial_session_name = initial_session_name(&args, &session_catalog)?;
    let initial_session_names = session_catalog.list()?;
    let (terminal_ui, ui_handle) = if interactive {
        let (ui, handle) = TerminalUi::new(
            network.id,
            product_id,
            initial_session_name.clone(),
            initial_session_names,
            initial_log_filter,
        );
        (Some(ui.enter()?), Some(handle))
    } else {
        (None, None)
    };
    let mut session = start_signing_host(
        &args,
        session_catalog,
        initial_session_name,
        network,
        ui_handle.clone(),
    )
    .await?;
    // Resolved before the port is bound, so a bad URL still fails on the argument
    // rather than half-way through startup - but reported below, once the UI
    // exists. A `tracing` line here goes to a stderr that the alternate screen
    // covers for the rest of the run, so in interactive mode nobody ever sees it.
    let debugger = debugger.map(connect_debugger).transpose()?;
    let frame_server = frame_server::bind(args.frame_listen).await?;
    let frame_url = frame_server.endpoint().to_string();
    terminal_ui::output_event(SystemEvent::FramesListening {
        url: frame_url.clone(),
    });
    if let Some(url) = bootstrap::bridge_url(&frame_url) {
        terminal_ui::output_event(SystemEvent::BridgeReady { url });
    }
    report_debugger(debugger.as_ref());
    let runtime_for_frames =
        tap_for_debugger(session.runtime_factory.clone(), debugger.map(|d| d.sink));

    if let Some(script) = args.script {
        let product_id = product.current();
        let script_product_id = product_id.clone();
        let script_frame_url = frame_url.clone();
        let initial_deeplink = args.deeplink.clone();
        let status = with_frame_server(runtime_for_frames, product, frame_server, async move {
            if let Some(deeplink) = initial_deeplink {
                start_deeplink_responder(&mut session, deeplink).await?;
            }
            ensure_signer(&mut session).await?;
            let status = script_runner::run(
                &script_frame_url,
                &script_product_id,
                &script,
                script_runner::ScriptHostRole::SigningHost,
            )
            .await?;
            session.responders.stop_all();
            Ok::<ExitStatus, anyhow::Error>(status)
        })
        .await?;
        let code = status.code().unwrap_or(1);
        terminal_ui::output_event(SystemEvent::ScriptExit { code });
        std::process::exit(code);
    }

    if args.serve {
        let serve_deeplink = args.deeplink.clone();
        let serve_frame_url = frame_url.clone();
        let auto_accept = args.auto_accept;
        return with_frame_server(
            runtime_for_frames,
            product.clone(),
            frame_server,
            async move {
                ensure_signer(&mut session).await?;
                restore_paired_responders(&mut session).await;
                if let Some(deeplink) = serve_deeplink {
                    start_deeplink_responder(&mut session, deeplink).await?;
                }
                terminal_ui::output_event(SystemEvent::ServeReady {
                    url: serve_frame_url,
                    auto_accept,
                });
                let code = match dev_command {
                    Some(command) => run_dev_command(command).await?,
                    None => {
                        wait_for_shutdown().await;
                        0
                    }
                };
                session.responders.stop_all();
                Ok(code)
            },
        )
        .await
        .map(|code| std::process::exit(code));
    }

    let initial_deeplink = args.deeplink.clone();
    if let Some(command) = exec_command {
        let cleanup_catalog = session.catalog.clone();
        let clear_target = with_frame_server(
            runtime_for_frames,
            product.clone(),
            frame_server,
            async move {
                if let Some(deeplink) = initial_deeplink {
                    start_deeplink_responder(&mut session, deeplink).await?;
                }
                let result = execute_non_interactive_command(
                    &mut session,
                    &frame_url,
                    &product,
                    command,
                    &log_controller,
                )
                .await;
                session.responders.stop_all();
                result
            },
        )
        .await?;
        if let Some(target) = clear_target {
            clear_sessions_after_shutdown(&cleanup_catalog, &target)?;
        }
        return Ok(());
    }

    let terminal_ui = terminal_ui.context("interactive terminal was not initialized")?;
    let initial_command = if let Some(deeplink) = initial_deeplink {
        Some((
            format!("/pair {deeplink}"),
            ShellCommand::Pair(PairCommand::Deeplink(deeplink)),
        ))
    } else {
        normalized(args.session.clone()).map(|name| {
            (
                format!("/session {name}"),
                ShellCommand::Session(SessionCommand::Switch(name)),
            )
        })
    };
    restore_paired_responders(&mut session).await;
    let cleanup_catalog = session.catalog.clone();
    let clear_target = with_frame_server(
        runtime_for_frames,
        product.clone(),
        frame_server,
        async move {
            signing_interactive_loop(
                &mut session,
                frame_url,
                product,
                initial_command,
                terminal_ui,
                log_controller,
            )
            .await
        },
    )
    .await?;
    if let Some(target) = clear_target {
        clear_sessions_after_shutdown(&cleanup_catalog, &target)?;
    }
    Ok(())
}

struct SigningHostSession {
    runtime: Arc<SigningHostRuntime>,
    platform: Arc<CliPlatform>,
    runtime_factory: Arc<frame_server::SwitchableSigningRuntime>,
    responders: ResponderManager,
    signer: Option<ResolvedSigner>,
    cached_user_id: Option<String>,
    last_script: Option<PathBuf>,
    script_projects: PathBuf,
    catalog: SessionCatalog,
    profile: Option<SessionProfile>,
    network: NetworkConfig,
    mnemonic: Option<String>,
    default_account: Option<String>,
    reserved_username: Option<String>,
    ui: Option<UiHandle>,
    /// Set when this host serves a chat product. Held across runtime rebuilds
    /// so switching session keeps the rooms and messages already posted.
    chat: Option<Arc<chat::CliChatHost>>,
    /// Set when this host serves a Pocket product. Held across runtime rebuilds
    /// so switching session keeps the card set it was seeded with.
    pocket: Option<Arc<pocket::CliPocketHost>>,
}

#[derive(Default)]
struct ResponderManager {
    tasks: HashMap<[u8; 32], tokio::task::JoinHandle<()>>,
}

impl ResponderManager {
    fn insert(&mut self, statement_account_id: [u8; 32], task: tokio::task::JoinHandle<()>) {
        if let Some(previous) = self.tasks.insert(statement_account_id, task) {
            previous.abort();
        }
    }

    fn remove(&mut self, statement_account_id: &[u8; 32]) -> bool {
        let Some(task) = self.tasks.remove(statement_account_id) else {
            return false;
        };
        task.abort();
        true
    }

    fn stop_all(&mut self) {
        for (_, task) in self.tasks.drain() {
            task.abort();
        }
    }
}

impl Drop for ResponderManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// The session a signing host starts in.
///
/// A name the caller chose is resolved through the aliases promotion records,
/// so a name that provisioned a session keeps selecting it instead of
/// provisioning a second identity beside it. Without a name, the base path
/// decides, and an ambiguous base path is refused rather than guessed.
fn initial_session_name(args: &SigningHostArgs, catalog: &SessionCatalog) -> Result<String> {
    if normalized(args.mnemonic.clone()).is_some() {
        return Ok("ephemeral".to_string());
    }
    if let Some(name) = normalized(args.session.clone()) {
        return catalog.resolve_session_name(&name);
    }
    if normalized(args.account.clone()).is_some() {
        return Ok(DEFAULT_SESSION_NAME.to_string());
    }
    match catalog.current_session()? {
        CurrentSession::Pointed(name) => Ok(name),
        CurrentSession::Recovered(name) => {
            tracing::warn!(
                session = %name,
                "selected session from its account store; the current-session pointer was missing or stale"
            );
            Ok(name)
        }
        CurrentSession::Fresh => Ok(DEFAULT_SESSION_NAME.to_string()),
        CurrentSession::Ambiguous { candidates } => Err(anyhow::anyhow!(
            "this base path holds several provisioned sessions ({}) and no current-session \
             pointer; name one with --session <name> rather than provisioning another identity",
            candidates.join(", "),
        )),
    }
}

fn selected_session_account(
    catalog: &SessionCatalog,
    profile: &SessionProfile,
    default_account: Option<&str>,
) -> Result<Option<String>> {
    Ok(catalog.cached_account_name(profile)?.or_else(|| {
        (profile.name == DEFAULT_SESSION_NAME)
            .then(|| default_account.map(str::to_string))
            .flatten()
    }))
}

async fn resolve_session_signer(config: ResolveSignerConfig<'_>) -> Result<ResolvedSigner> {
    if config.mnemonic.is_none()
        && let Some(signer) = accounts::resolve_cached_signer(
            config.base_path,
            config.network.id,
            config.account.as_deref(),
        )?
    {
        return Ok(signer);
    }
    accounts::resolve_signer(config).await
}

async fn start_signing_host(
    args: &SigningHostArgs,
    catalog: SessionCatalog,
    session_name: String,
    network: NetworkConfig,
    ui: Option<UiHandle>,
) -> Result<SigningHostSession> {
    let mnemonic = normalized(args.mnemonic.clone());
    let mut profile = if mnemonic.is_some() {
        None
    } else {
        Some(catalog.ensure_profile(&session_name)?)
    };
    let default_account = normalized(args.account.clone());
    let mut cached_user_id = profile
        .as_ref()
        .map(|profile| catalog.cached_user_id(profile))
        .transpose()?
        .flatten();
    let selected_account = profile
        .as_ref()
        .map(|profile| selected_session_account(&catalog, profile, default_account.as_deref()))
        .transpose()?
        .flatten();
    let mut signer = profile
        .as_ref()
        .map(|profile| {
            accounts::resolve_cached_signer(
                &profile.account_base_path,
                network.id,
                selected_account.as_deref(),
            )
        })
        .transpose()?
        .flatten();
    let promotion = if let (Some(current), Some(user_id)) = (
        &profile,
        signer
            .as_ref()
            .and_then(|signer| signer.lite_username.as_ref()),
    ) && current.name != *user_id
    {
        let promotion = catalog.prepare_promotion(current, user_id)?;
        profile = Some(promotion.profile().clone());
        cached_user_id = Some(user_id.clone());
        Some(promotion)
    } else {
        None
    };
    let storage_profile = profile.as_ref().cloned().unwrap_or_else(|| {
        catalog
            .profile(DEFAULT_SESSION_NAME)
            .expect("default session profile is valid")
    });
    if signer.is_none() && mnemonic.is_some() {
        let mut explicit_signer = accounts::resolve_signer(ResolveSignerConfig {
            base_path: &storage_profile.account_base_path,
            network,
            mnemonic: mnemonic.clone(),
            account: None,
            lite_username_prefix: None,
            reserved_username: None,
        })
        .await?;
        match attestation::registered_lite_username(network, &explicit_signer.entropy).await {
            Ok(user_id) => explicit_signer.lite_username = Some(user_id),
            Err(error) => {
                tracing::warn!(%error, "explicit signer has no resolvable dotNS username")
            }
        }
        signer = Some(explicit_signer);
    }
    let approval = approval_policy(args.auto_accept);
    let chat = args.execution_kind.chat_host();
    let pocket = args.execution_kind.pocket_host();
    let (runtime, platform) = build_signing_runtime(
        network,
        storage_profile.path,
        storage_profile.product_storage_dir,
        approval,
        ui.clone(),
        chat.clone(),
        pocket.clone(),
    )?;
    apply_local_product_grants(platform.as_ref(), &args.product_config).await?;
    let runtime_factory = frame_server::SwitchableSigningRuntime::new(runtime.clone());
    let last_script = profile
        .as_ref()
        .map(|profile| catalog.last_script(profile))
        .transpose()?
        .flatten();
    if let Some(cached_signer) = &signer {
        runtime
            .activate_local_session_with_identity(
                cached_signer.entropy.clone(),
                cached_signer.lite_username.clone(),
            )
            .await
            .map_err(|error| {
                anyhow::anyhow!("failed to activate cached session: {}", error.reason)
            })?;
        if let (Some(profile), Some(user_id)) = (&profile, &cached_signer.lite_username) {
            if let Some(account_name) = &cached_signer.account_name {
                catalog.store_signer_binding(profile, user_id, account_name)?;
            } else {
                catalog.store_user_id(profile, user_id)?;
            }
            cached_user_id = Some(user_id.clone());
            if let Some(ui) = &ui {
                ui.connection(user_id.clone());
            }
        }
        terminal_ui::output_event(SystemEvent::SigningHostReady);
    }
    if let Some(profile) = &profile {
        if profile.name != session_name
            && let Some(ui) = &ui
        {
            ui.session(profile.name.clone(), catalog.list()?);
        }
        if signer.is_some() || normalized(args.session.clone()).is_none() || args.action.is_some() {
            catalog.set_current(&profile.name)?;
        }
    }
    if profile.is_some() && signer.is_none() {
        terminal_ui::output_event(SystemEvent::SigningHostNeedsSession);
    }
    if let Some(promotion) = promotion {
        promotion.commit();
    }

    Ok(SigningHostSession {
        runtime,
        platform,
        runtime_factory,
        responders: ResponderManager::default(),
        signer,
        cached_user_id,
        last_script,
        script_projects: script_project_directory(args.base_path.clone()),
        catalog,
        profile,
        network,
        mnemonic,
        default_account,
        reserved_username: normalized(args.reserved_username.clone()),
        ui,
        chat,
        pocket,
    })
}

fn build_signing_runtime(
    network: NetworkConfig,
    storage_path: PathBuf,
    product_storage_dir: PathBuf,
    approval: ApprovalPolicy,
    ui: Option<UiHandle>,
    chat: Option<Arc<chat::CliChatHost>>,
    pocket: Option<Arc<pocket::CliPocketHost>>,
) -> Result<(Arc<SigningHostRuntime>, Arc<CliPlatform>)> {
    std::fs::create_dir_all(&storage_path)
        .with_context(|| format!("creating state directory {}", storage_path.display()))?;
    let core_db = futures::executor::block_on(truapi::store::Db::open(
        truapi::store::core_db_config(&storage_path),
    ))
    .with_context(|| format!("opening the core database in {}", storage_path.display()))?;
    let platform = CliPlatform::new(
        network,
        Some(CliStoragePaths::new(storage_path, product_storage_dir)),
        approval,
        ui,
    );
    let config = SigningHostConfig::new(
        host_info("Headless Signing Host"),
        platform_info(),
        network.people_genesis,
        network.bulletin_genesis,
        network.asset_hub_genesis,
        network.network_suffix.to_string(),
    )
    .context("invalid signing host config")?;
    let status_host = platform.clone() as Arc<dyn PermissionStatusHost>;
    let runtime = Arc::new(SigningHostRuntime::with_chat_platform(
        platform.clone(),
        config,
        tokio_spawner(),
        chat.map(|chat| chat as Arc<dyn ChatPlatform>),
    ));
    runtime.set_permission_status_host(status_host);
    if let Some(pocket) = pocket {
        runtime.set_pocket_platform(pocket);
    }
    runtime.set_core_db(core_db);
    runtime.start_statement_allowance_renewal();
    Ok((runtime, platform))
}

impl Drop for SigningHostSession {
    fn drop(&mut self) {
        self.responders.stop_all();
    }
}

fn validate_signing_args(args: &SigningHostArgs) -> Result<()> {
    let mnemonic = normalized(args.mnemonic.clone());
    let account = normalized(args.account.clone());
    let session = normalized(args.session.clone());
    if args.script.is_some() && args.action.is_some() {
        bail!("--script cannot be combined with the exec subcommand");
    }
    if args.serve && args.script.is_some() {
        bail!("--serve cannot be combined with --script");
    }
    if args.serve && args.action.is_some() {
        bail!("--serve cannot be combined with the exec subcommand");
    }
    if mnemonic.is_some() && account.is_some() {
        bail!("--account cannot be used when --mnemonic or HOST_CLI_SIGNER_MNEMONIC is set");
    }
    if mnemonic.is_some() && session.is_some() {
        bail!("--session cannot be used when --mnemonic or HOST_CLI_SIGNER_MNEMONIC is set");
    }
    if mnemonic.is_some()
        && args.action.as_ref().is_some_and(|action| {
            let SigningHostAction::Exec { command } = action;
            matches!(parse_command(command), Ok(ShellCommand::Devices(_)))
        })
    {
        bail!("paired-device management is unavailable when launched with --mnemonic");
    }
    if account.is_some() && session.is_some() {
        bail!("--session cannot be combined with --account");
    }
    if let Some(session) = session {
        sessions::validate_selectable_name(&session).map_err(anyhow::Error::msg)?;
    }
    let reserved = normalized(args.reserved_username.clone());
    if mnemonic.is_some() && reserved.is_some() {
        bail!(
            "--reserved-username cannot be used when --mnemonic or HOST_CLI_SIGNER_MNEMONIC is set"
        );
    }
    if account.is_some() && reserved.is_some() {
        bail!("--reserved-username only applies when --account is omitted");
    }
    if let Some(reserved) = &reserved
        && !is_registrable_full_label(reserved)
    {
        bail!(
            "--reserved-username {reserved:?} is not a reservable base label: lowercase ASCII \
             letters only, {MIN_PERSON_LABEL_LEN} to {MAX_BASE_LABEL_LEN} bytes"
        );
    }
    Ok(())
}

fn normalized(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        (!trimmed.is_empty()).then_some(trimmed)
    })
}

fn invalid_invocation(error: impl fmt::Display) -> ! {
    eprintln!("error: {error}");
    std::process::exit(2);
}

async fn with_frame_server<T, Fut>(
    runtime: Arc<dyn frame_server::ProductRuntimeFactory>,
    product: Arc<frame_server::ProductSelection>,
    frame_server: frame_server::BoundFrameServer,
    body: Fut,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let server = tokio::spawn(frame_server::accept_loop(runtime, product, frame_server));
    let result = body.await;
    server.abort();
    let _ = server.await;
    result
}

fn paired_host_from_deeplink(deeplink: &str) -> Result<PairedHost> {
    let proposal = PairingProposal::from_deeplink(deeplink).map_err(anyhow::Error::msg)?;
    Ok(PairedHost::new(
        proposal.peer.statement_account_id,
        proposal.peer.encryption_public_key,
        PairedHostMetadata {
            host_name: proposal.metadata.host_name,
            host_version: proposal.metadata.host_version,
            host_icon: proposal.metadata.host_icon,
            platform_type: proposal.metadata.platform_type,
            platform_version: proposal.metadata.platform_version,
        },
    ))
}

fn paired_sso_peer(host: &PairedHost) -> PairedSsoPeer {
    PairedSsoPeer {
        statement_account_id: host.statement_account_id(),
        encryption_public_key: host.encryption_public_key(),
    }
}

fn persist_paired_host(session: &SigningHostSession, host: &PairedHost) -> Result<()> {
    if let Some(profile) = &session.profile {
        session.catalog.store_paired_host(profile, host.clone())?;
    }
    Ok(())
}

fn spawn_supervised_responder(
    runtime: Arc<SigningHostRuntime>,
    host: PairedHost,
    persisted: Option<(SessionCatalog, SessionProfile)>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let peer = paired_sso_peer(&host);
        let mut retry_delay = std::time::Duration::from_secs(1);
        loop {
            let result = runtime.resume_pairing(peer).await;
            match result {
                Ok(ResponderExit::PeerDisconnected) => {
                    if let Some((catalog, profile)) = &persisted {
                        loop {
                            match catalog.remove_paired_host(profile, &peer.statement_account_id) {
                                Ok(_) => break,
                                Err(error) => {
                                    terminal_ui::output_event(SystemEvent::SigningHostError {
                                        reason: format!(
                                            "failed to remove disconnected paired device: {error}"
                                        ),
                                    });
                                    tokio::time::sleep(retry_delay).await;
                                    retry_delay =
                                        (retry_delay * 2).min(std::time::Duration::from_secs(30));
                                }
                            }
                        }
                        if let Err(error) = runtime
                            .untrack_statement_renewal_account(&peer.statement_account_id)
                            .await
                        {
                            terminal_ui::output_event(SystemEvent::SigningHostError {
                                reason: format!(
                                    "failed to stop renewing disconnected paired device: {}",
                                    error.reason
                                ),
                            });
                        }
                    }
                    terminal_ui::output_event(SystemEvent::SigningHostExit {
                        outcome: format!("{:?}", ResponderExit::PeerDisconnected),
                    });
                    return;
                }
                Ok(ResponderExit::SubscriptionEnded) => {
                    terminal_ui::output_event(SystemEvent::SigningHostExit {
                        outcome: format!("{:?}", ResponderExit::SubscriptionEnded),
                    });
                }
                Err(error) => {
                    terminal_ui::output_event(SystemEvent::SigningHostError {
                        reason: error.reason,
                    });
                }
            }
            tokio::time::sleep(retry_delay).await;
            retry_delay = (retry_delay * 2).min(std::time::Duration::from_secs(30));
        }
    })
}

fn start_paired_host_responder(session: &mut SigningHostSession, host: PairedHost) {
    let statement_account_id = host.statement_account_id();
    let persisted = session
        .profile
        .clone()
        .map(|profile| (session.catalog.clone(), profile));
    let task = spawn_supervised_responder(session.runtime.clone(), host, persisted);
    session.responders.insert(statement_account_id, task);
    terminal_ui::output_event(SystemEvent::SigningHostResponderStarted);
}

async fn restore_paired_responders(session: &mut SigningHostSession) {
    if session.signer.is_none() {
        return;
    }
    let Some(profile) = session.profile.clone() else {
        return;
    };
    let paired_hosts = match session.catalog.paired_hosts(&profile) {
        Ok(paired_hosts) => paired_hosts,
        Err(error) => {
            tracing::warn!(%error, "failed to read saved paired devices");
            return;
        }
    };
    if paired_hosts.is_empty() {
        return;
    }
    let mut renewal_targets = vec![StatementRenewalTarget::WalletSso];
    renewal_targets.extend(
        paired_hosts
            .iter()
            .map(|host| pairing_device_renewal_target(host.statement_account_id())),
    );
    if let Err(error) = session
        .runtime
        .track_statement_renewal_targets(renewal_targets)
        .await
    {
        tracing::warn!(
            reason = %error.reason,
            "failed to restore paired-device allowance renewal"
        );
    }
    for paired_host in paired_hosts {
        start_paired_host_responder(session, paired_host);
    }
}

/// How long the wrapped development command has to exit after termination is
/// requested before it is killed.
const DEV_COMMAND_GRACE: Duration = Duration::from_secs(5);

/// Conventional exit code for a process stopped by an interrupt.
const INTERRUPTED_EXIT_CODE: i32 = 130;

/// Run a product against a local signing host with one command.
///
/// Everything the host and the product need to agree on is derived here, so
/// they cannot disagree: the product id names the development server's own
/// origin, and the bridge script the product loads is generated by this
/// process from the endpoint it just bound.
/// Apply the grants each local product config declares, and say that they are
/// local.
///
/// The core resolves these through the same manifest path it uses on chain, so
/// what a developer sees here is what the published manifest will do. What it
/// cannot tell them is that the manifest is not published yet, so the host says
/// it every run: a grant that works locally and was never deployed is the
/// failure this feature would otherwise cause.
async fn apply_local_product_grants(platform: &CliPlatform, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let configs = product_config::read_all(paths)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    let applied = product_config::apply(platform, &configs, now).await?;
    if applied.is_empty() {
        return Ok(());
    }
    tracing::info!("Local product grants — declared in config, not published to dotNS:");
    for line in &applied.lines {
        tracing::info!("  {line}");
    }
    Ok(())
}

async fn run_dev(
    args: DevArgs,
    initial_log_filter: String,
    log_controller: LogController,
    debugger: Option<DebuggerSwitch>,
) -> Result<()> {
    bootstrap::read_container(&bootstrap::container_path())?;
    let product_id = args
        .product_id
        .unwrap_or_else(|| format!("localhost:{}", args.app_port));
    let signing = SigningHostArgs {
        product_id,
        network: args.network,
        session: args.session,
        mnemonic: args.mnemonic,
        base_path: args.base_path,
        product_config: args.product_config,
        frame_listen: Some(SocketAddr::from((Ipv4Addr::LOCALHOST, args.port))),
        // A process with no terminal cannot prompt, which is why this pairs
        // with a testnet-only network preset.
        serve: true,
        auto_accept: true,
        ..Default::default()
    };
    let command = (!args.command.is_empty()).then_some(args.command);
    run_signing_host(
        signing,
        initial_log_filter,
        log_controller,
        command,
        debugger,
    )
    .await
}

/// Run the wrapped development command, returning the code to exit with.
///
/// The command owns a process group whose id is retained after its direct child
/// exits, because package managers can leave the actual development server
/// running two or three intermediate processes away.
async fn run_dev_command(command: Vec<String>) -> Result<i32> {
    let (program, arguments) = command
        .split_first()
        .expect("an empty dev command is never supervised");
    let mut command = DevCommand::spawn(program, arguments)?;

    tokio::select! {
        status = command.child.wait() => {
            let exit_code = status?.code().unwrap_or(1);
            stop_dev_command(&mut command).await;
            Ok(exit_code)
        }
        _ = wait_for_shutdown() => {
            stop_dev_command(&mut command).await;
            let _ = command.child.wait().await;
            // The command was interrupted, not failed. Report the interrupt
            // rather than whatever a terminated package manager last said.
            Ok(INTERRUPTED_EXIT_CODE)
        }
    }
}

struct DevCommand {
    child: tokio::process::Child,
    #[cfg(unix)]
    process_group: rustix::process::Pid,
}

impl DevCommand {
    fn spawn(program: &str, arguments: &[String]) -> Result<Self> {
        let mut command = tokio::process::Command::new(program);
        command.args(arguments);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;

            command.as_std_mut().process_group(0);
        }
        let child = command
            .spawn()
            .with_context(|| format!("failed to start {program}"))?;
        #[cfg(unix)]
        let process_group = child
            .id()
            .and_then(|pid| i32::try_from(pid).ok())
            .and_then(rustix::process::Pid::from_raw)
            .context("development command has no process group id")?;
        Ok(Self {
            child,
            #[cfg(unix)]
            process_group,
        })
    }
}

/// Terminate the development command and everything it spawned.
#[cfg(unix)]
async fn stop_dev_command(command: &mut DevCommand) {
    use rustix::process::{Signal, kill_process_group};

    let _ = kill_process_group(command.process_group, Signal::TERM);
    if wait_for_dev_process_group_exit(command).await {
        return;
    }
    let _ = kill_process_group(command.process_group, Signal::KILL);
    let _ = wait_for_dev_process_group_exit(command).await;
}

#[cfg(unix)]
async fn wait_for_dev_process_group_exit(command: &mut DevCommand) -> bool {
    let exited = async {
        loop {
            let _ = command.child.try_wait();
            if matches!(
                rustix::process::test_kill_process_group(command.process_group),
                Err(rustix::io::Errno::SRCH)
            ) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(DEV_COMMAND_GRACE, exited)
        .await
        .is_ok()
}

#[cfg(not(unix))]
async fn stop_dev_command(command: &mut DevCommand) {
    let _ = command.child.start_kill();
}

/// Resolve once the process is asked to stop.
///
/// Termination counts alongside interruption: a supervisor stopping this
/// process (a test runner, systemd, a container) sends `SIGTERM`, and the
/// shutdown path is what releases responders and stops the development command,
/// which runs in its own process group and so never sees the signal itself.
async fn wait_for_shutdown() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = wait_for_interrupt() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(error) => {
                terminal_ui::output_event(SystemEvent::SigningHostError {
                    reason: format!("SIGTERM handling unavailable: {error}"),
                });
                wait_for_interrupt().await;
            }
        }
    }
    #[cfg(not(unix))]
    wait_for_interrupt().await;
}

async fn wait_for_interrupt() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        // Nothing to await without a signal handler, so stay up and serve until
        // the process is killed rather than exiting as soon as it starts.
        terminal_ui::output_event(SystemEvent::SigningHostError {
            reason: format!("ctrl-c handling unavailable: {error}"),
        });
        std::future::pending::<()>().await;
    }
}

async fn ensure_signer(session: &mut SigningHostSession) -> Result<()> {
    if session.signer.is_some() {
        return Ok(());
    }
    if let Some(profile) = &session.profile {
        activate_session(session, profile.name.clone()).await?;
        terminal_ui::output_event(SystemEvent::SigningHostReady);
        return Ok(());
    }
    let profile = session.catalog.profile(DEFAULT_SESSION_NAME)?;
    session.signer = Some(
        resolve_session_signer(ResolveSignerConfig {
            base_path: &profile.account_base_path,
            network: session.network,
            mnemonic: session.mnemonic.clone(),
            account: None,
            lite_username_prefix: None,
            reserved_username: None,
        })
        .await?,
    );
    if let Err(error) = activate_current_signer(session).await {
        session.signer = None;
        return Err(error);
    }
    Ok(())
}

fn current_account_base_path(session: &SigningHostSession) -> Result<PathBuf> {
    Ok(session
        .profile
        .as_ref()
        .map(|profile| profile.account_base_path.clone())
        .unwrap_or(
            session
                .catalog
                .profile(DEFAULT_SESSION_NAME)?
                .account_base_path,
        ))
}

async fn activate_current_signer(session: &mut SigningHostSession) -> Result<()> {
    let signer = session
        .signer
        .as_ref()
        .context("signer has not been resolved")?;
    session
        .runtime
        .activate_local_session_with_identity(signer.entropy.clone(), signer.lite_username.clone())
        .await
        .map_err(|err| anyhow::anyhow!("failed to activate local session: {}", err.reason))?;
    if let (Some(profile), Some(user_id)) = (&session.profile, &signer.lite_username) {
        if let Some(account_name) = &signer.account_name {
            session
                .catalog
                .store_signer_binding(profile, user_id, account_name)?;
        } else {
            session.catalog.store_user_id(profile, user_id)?;
        }
        session.cached_user_id = Some(user_id.clone());
        if let Some(ui) = &session.ui {
            ui.connection(user_id.clone());
        }
    }
    terminal_ui::output_event(SystemEvent::SigningHostReady);
    Ok(())
}

/// Register this host's own statement-store allowance, reporting whether it
/// ended up usable.
///
/// Every handshake answer is signed by the `WalletSso` account, so nothing
/// reaches the pairing host until this lands. Failing is not fatal: the
/// device-slot pass renews the same target, and can rotate the signer to get
/// it. It only means there is no way to say anything in the meantime.
async fn prepare_own_allowance(session: &mut SigningHostSession) -> bool {
    use truapi::statement_allowance::renewal::TargetRenewalStatus;

    if let Err(error) = ensure_signer(session).await {
        tracing::warn!(%error, "no signer to register the wallet allowance under");
        return false;
    }
    if let Err(error) = session
        .runtime
        .track_statement_renewal_targets(vec![StatementRenewalTarget::WalletSso])
        .await
    {
        tracing::warn!(reason = %error.reason, "failed to record the wallet allowance");
        return false;
    }
    let report = match session.runtime.renew_statement_allowances().await {
        Ok(report) => report,
        Err(error) => {
            tracing::warn!(reason = %error.reason, "wallet allowance renewal failed");
            return false;
        }
    };
    // `renew_statement_allowances` reports per-target failures in its outcomes
    // rather than as an error, so an exhausted slot reads as success here
    // unless the outcome itself is checked.
    report
        .outcomes
        .iter()
        .rev()
        .find(|outcome| outcome.label == WALLET_SSO_RENEWAL_LABEL)
        .is_some_and(|outcome| {
            matches!(
                outcome.status,
                TargetRenewalStatus::Registered { .. }
                    | TargetRenewalStatus::AlreadyAllocated { .. }
            )
        })
}

/// Tell the pairing host that allocation has started, so it stops showing a QR
/// that has already been scanned.
async fn announce_allowance_allocation(
    session: &mut SigningHostSession,
    deeplink: &str,
) -> Option<AnnouncedPairing> {
    if !prepare_own_allowance(session).await {
        return None;
    }
    match session
        .runtime
        .notify_pairing_allowance_allocation(deeplink)
        .await
    {
        Ok(announced) => Some(announced),
        Err(error) => {
            terminal_ui::output_event(SystemEvent::SigningHostError {
                reason: format!(
                    "failed to announce allowance allocation to the pairing host: {}",
                    error.reason
                ),
            });
            None
        }
    }
}

/// Tell a pairing host that already dropped its QR why pairing stopped.
///
/// It waits on the handshake topic without a deadline, so staying silent here
/// leaves it waiting forever. Reported under `announced`, because allocating
/// the device slot can rotate this host's signer onto an account that never
/// obtained an allowance and so cannot reach the host at all.
async fn report_pairing_failure(
    session: &SigningHostSession,
    announced: &AnnouncedPairing,
    reason: String,
) {
    if let Err(error) = session
        .runtime
        .notify_pairing_failed(announced, reason)
        .await
    {
        terminal_ui::output_event(SystemEvent::SigningHostError {
            reason: format!(
                "failed to tell the pairing host that pairing failed: {}",
                error.reason
            ),
        });
    }
}

async fn prepare_pairing_response(
    session: &mut SigningHostSession,
    candidate: &PairedHost,
    existing: &[PairedHost],
) -> Result<()> {
    let mut attempts = 0usize;
    loop {
        ensure_signer(session).await?;
        let (auto_managed, account_name) = {
            let signer = session
                .signer
                .as_ref()
                .context("signer has not been resolved")?;
            (signer.auto_managed, signer.account_name.clone())
        };
        match renew_pairing_allowances(session, existing, candidate).await {
            Ok(()) => return Ok(()),
            Err(err)
                if signer_identity_may_rotate(auto_managed, existing.len())
                    && is_statement_slot_exhaustion(&err) =>
            {
                attempts += 1;
                if attempts > 8 {
                    return Err(err);
                }
                if let Some(name) = &account_name {
                    let period = accounts::current_statement_period()?;
                    let account_base_path = current_account_base_path(session)?;
                    accounts::mark_account_exhausted(
                        &account_base_path,
                        session.network.id,
                        name,
                        period,
                    )?;
                    terminal_ui::output_event(SystemEvent::SigningHostAccountExhausted {
                        name: name.clone(),
                        period,
                    });
                }
                let account_base_path = current_account_base_path(session)?;
                let session_name = session
                    .profile
                    .as_ref()
                    .map_or(DEFAULT_SESSION_NAME, |profile| profile.name.as_str());
                let lite_username_prefix = sessions::lite_username_prefix(session_name);
                session.signer = Some(
                    accounts::resolve_signer(ResolveSignerConfig {
                        base_path: &account_base_path,
                        network: session.network,
                        mnemonic: None,
                        account: None,
                        lite_username_prefix,
                        reserved_username: session.reserved_username.clone(),
                    })
                    .await?,
                );
                activate_current_signer(session).await?;
            }
            Err(err) if !existing.is_empty() && is_statement_slot_exhaustion(&err) => {
                return Err(err).context(
                    "cannot pair another device without replacing an existing pairing; remove a paired device or wait for a new allowance period",
                );
            }
            Err(err) => return Err(err),
        }
    }
}

async fn establish_paired_host(
    session: &mut SigningHostSession,
    deeplink: &str,
) -> Result<PairedHost> {
    let candidate = paired_host_from_deeplink(deeplink)?;
    let existing = session
        .profile
        .as_ref()
        .map(|profile| session.catalog.paired_hosts(profile))
        .transpose()?
        .unwrap_or_default();
    let candidate_is_existing = existing
        .iter()
        .any(|host| host.statement_account_id() == candidate.statement_account_id());
    // Allocating the device slot submits extrinsics and waits for them on
    // chain. Without this the pairing host stays on its QR screen throughout,
    // with no sign that the scan registered.
    let announced = announce_allowance_allocation(session, deeplink).await;

    if let Err(error) = prepare_pairing_response(session, &candidate, &existing).await {
        if let Some(announced) = &announced {
            report_pairing_failure(session, announced, error.to_string()).await;
        }
        discard_new_pairing_candidate(session, &candidate, candidate_is_existing).await;
        return Err(error);
    }

    let result = async {
        session
            .runtime
            .establish_pairing(deeplink)
            .await
            .map_err(|error| anyhow::anyhow!("pairing failed: {}", error.reason))?;
        persist_paired_host(session, &candidate)
    }
    .await;
    if let Err(error) = result {
        if let Some(announced) = &announced {
            report_pairing_failure(session, announced, error.to_string()).await;
        }
        discard_new_pairing_candidate(session, &candidate, candidate_is_existing).await;
        return Err(error);
    }
    Ok(candidate)
}

async fn discard_new_pairing_candidate(
    session: &SigningHostSession,
    candidate: &PairedHost,
    candidate_is_existing: bool,
) {
    if !candidate_is_existing
        && let Err(error) = session
            .runtime
            .untrack_statement_renewal_account(&candidate.statement_account_id())
            .await
    {
        tracing::warn!(
            reason = %error.reason,
            "failed to discard abandoned pairing allowance renewal"
        );
    }
}

fn is_statement_slot_exhaustion(err: &anyhow::Error) -> bool {
    truapi::reports_exhausted_period(&err.to_string())
}

fn signer_identity_may_rotate(auto_managed: bool, paired_host_count: usize) -> bool {
    auto_managed && paired_host_count == 0
}

/// Report label `StatementRenewalTarget::WalletSso` renews under.
const WALLET_SSO_RENEWAL_LABEL: &str = "wallet-sso";

fn pairing_device_renewal_target(statement_account_id: [u8; 32]) -> StatementRenewalTarget {
    StatementRenewalTarget::Account {
        account_id: statement_account_id,
        label: format!("device:0x{}", hex::encode(statement_account_id)),
    }
}

async fn renew_pairing_allowances(
    session: &SigningHostSession,
    existing: &[PairedHost],
    candidate: &PairedHost,
) -> Result<()> {
    use truapi::statement_allowance::renewal::TargetRenewalStatus;

    let candidate_id = candidate.statement_account_id();
    let candidate_is_existing = existing
        .iter()
        .any(|host| host.statement_account_id() == candidate_id);
    let mut required_device_ids = existing
        .iter()
        .map(PairedHost::statement_account_id)
        .collect::<Vec<_>>();
    if !candidate_is_existing {
        required_device_ids.push(candidate_id);
    }
    required_device_ids.sort();
    required_device_ids.dedup();

    let mut targets = vec![StatementRenewalTarget::WalletSso];
    targets.extend(
        required_device_ids
            .iter()
            .copied()
            .map(pairing_device_renewal_target),
    );
    session
        .runtime
        .track_statement_renewal_targets(targets)
        .await
        .map_err(|error| {
            anyhow::anyhow!("failed to record pairing allowances: {}", error.reason)
        })?;
    let report = match session.runtime.renew_statement_allowances().await {
        Ok(report) => report,
        Err(error) => {
            if !candidate_is_existing {
                let _ = session
                    .runtime
                    .untrack_statement_renewal_account(&candidate_id)
                    .await;
            }
            bail!("pairing allowance renewal failed: {}", error.reason);
        }
    };

    let mut required_labels = vec![WALLET_SSO_RENEWAL_LABEL.to_string()];
    required_labels.extend(
        required_device_ids
            .iter()
            .map(|account_id| format!("device:0x{}", hex::encode(account_id))),
    );
    for label in required_labels {
        let status = report
            .outcomes
            .iter()
            .rev()
            .find(|outcome| outcome.label == label)
            .map(|outcome| &outcome.status)
            .with_context(|| format!("pairing allowance renewal omitted {label}"))?;
        match status {
            TargetRenewalStatus::Registered { .. }
            | TargetRenewalStatus::AlreadyAllocated { .. } => {}
            TargetRenewalStatus::Failed { reason } => {
                if !candidate_is_existing {
                    let _ = session
                        .runtime
                        .untrack_statement_renewal_account(&candidate_id)
                        .await;
                }
                bail!("pairing allowance renewal for {label} failed: {reason}");
            }
            TargetRenewalStatus::SkippedExhausted => {
                if !candidate_is_existing {
                    let _ = session
                        .runtime
                        .untrack_statement_renewal_account(&candidate_id)
                        .await;
                }
                bail!("no free StatementStore slot for {label}");
            }
        }
    }
    Ok(())
}

/// Renew tracked statement-store allowances now, reporting each target.
async fn run_renew(session: &mut SigningHostSession) -> Result<()> {
    use truapi::statement_allowance::renewal::TargetRenewalStatus;

    ensure_signer(session).await?;
    let report = session
        .runtime
        .renew_statement_allowances()
        .await
        .map_err(|err| anyhow::anyhow!("allowance renewal failed: {}", err.reason))?;

    let (mut renewed, mut fresh, mut failed, mut skipped) = (0usize, 0usize, 0usize, 0usize);
    for outcome in &report.outcomes {
        let target = &outcome.label;
        match &outcome.status {
            TargetRenewalStatus::Registered { seq, block_hash } => {
                renewed += 1;
                terminal_ui::output_event(SystemEvent::AllowanceReady {
                    target: target.clone(),
                    sequence: *seq,
                    block_hash: Some(block_hash.clone()),
                    already_allocated: false,
                });
            }
            TargetRenewalStatus::AlreadyAllocated { seq } => {
                fresh += 1;
                terminal_ui::output_event(SystemEvent::AllowanceReady {
                    target: target.clone(),
                    sequence: *seq,
                    block_hash: None,
                    already_allocated: true,
                });
            }
            TargetRenewalStatus::Failed { reason } => {
                failed += 1;
                terminal_ui::output_event(SystemEvent::AllowanceRenewalFailed {
                    target: target.clone(),
                    reason: reason.clone(),
                });
            }
            TargetRenewalStatus::SkippedExhausted => skipped += 1,
        }
    }
    for label in &report.pruned {
        terminal_ui::output_event(SystemEvent::AllowanceRenewalPruned {
            target: label.clone(),
        });
    }
    terminal_ui::output_event(SystemEvent::AllowanceRenewalReport {
        period: report.period,
        renewed,
        fresh,
        failed,
        skipped,
        pruned: report.pruned.len(),
    });

    if report.slots_exhausted {
        let has_paired_hosts = session
            .profile
            .as_ref()
            .map(|profile| session.catalog.paired_hosts(profile))
            .transpose()?
            .is_some_and(|paired_hosts| !paired_hosts.is_empty());
        if has_paired_hosts {
            tracing::warn!(
                "statement-store slots are exhausted; preserving the signer because paired devices depend on its identity"
            );
        } else {
            mark_current_account_exhausted(session)?;
        }
    }
    Ok(())
}

fn mark_current_account_exhausted(session: &SigningHostSession) -> Result<()> {
    let Some(signer) = session.signer.as_ref() else {
        return Ok(());
    };
    if !signer.auto_managed {
        return Ok(());
    }
    let Some(name) = signer.account_name.clone() else {
        return Ok(());
    };
    let period = accounts::current_statement_period()?;
    let account_base_path = current_account_base_path(session)?;
    accounts::mark_account_exhausted(&account_base_path, session.network.id, &name, period)?;
    terminal_ui::output_event(SystemEvent::SigningHostAccountExhausted { name, period });
    Ok(())
}

async fn respond_to_deeplink(session: &mut SigningHostSession, deeplink: String) -> Result<()> {
    let host = establish_paired_host(session, &deeplink).await?;
    let exit = session
        .runtime
        .resume_pairing(paired_sso_peer(&host))
        .await
        .map_err(|err| anyhow::anyhow!("pairing failed: {}", err.reason))?;
    if exit == ResponderExit::PeerDisconnected && session.profile.is_some() {
        remove_paired_host_locally(session, &host.statement_account_id()).await?;
    }
    terminal_ui::output_event(SystemEvent::SigningHostExit {
        outcome: format!("{exit:?}"),
    });
    Ok(())
}

async fn start_deeplink_responder(
    session: &mut SigningHostSession,
    deeplink: String,
) -> Result<()> {
    let host = establish_paired_host(session, &deeplink).await?;
    start_paired_host_responder(session, host);
    Ok(())
}

fn find_paired_host(
    session: &SigningHostSession,
    statement_account_id: &[u8; 32],
) -> Result<PairedHost> {
    let profile = session
        .profile
        .as_ref()
        .context("paired-device management is unavailable when launched with --mnemonic")?;
    session
        .catalog
        .paired_hosts(profile)?
        .into_iter()
        .find(|host| host.statement_account_id() == *statement_account_id)
        .with_context(|| {
            format!(
                "paired device 0x{} does not exist in session {}; use /devices to list paired devices",
                hex::encode(statement_account_id),
                profile.name
            )
        })
}

struct PairedHostRemoval {
    paired_host: PairedHost,
    notification_failure: Option<String>,
}

/// Submit the disconnect first so ordinary removal never deletes an unnotified
/// pairing. A later cleanup failure remains retryable even though the peer may
/// already consider the session closed.
async fn disconnect_and_remove_paired_host(
    session: &mut SigningHostSession,
    statement_account_id: &[u8; 32],
    force: bool,
) -> Result<PairedHostRemoval> {
    let paired_host = find_paired_host(session, statement_account_id)?;
    let notification = tokio::time::timeout(
        Duration::from_secs(30),
        session
            .runtime
            .disconnect_paired_host(paired_sso_peer(&paired_host)),
    )
    .await
    .map_err(|_| "disconnect notification submission timed out".to_string())
    .and_then(|result| result.map_err(|error| error.reason));
    let notification_failure = match notification {
        Ok(()) => None,
        Err(reason) if force => Some(reason),
        Err(reason) => {
            bail!("failed to notify paired device before removal: {reason}")
        }
    };
    remove_paired_host_locally(session, statement_account_id).await?;
    Ok(PairedHostRemoval {
        paired_host,
        notification_failure,
    })
}

fn forced_removal_warning(reason: &str) -> (&'static str, String) {
    (
        "Paired device removed without notification",
        format!(
            "Notification failed: {reason}. Forced local removal completed. The remote host may still show stale connected state, but it cannot reach a responder on this signing host."
        ),
    )
}

async fn remove_paired_host_locally(
    session: &mut SigningHostSession,
    statement_account_id: &[u8; 32],
) -> Result<()> {
    let profile = session
        .profile
        .as_ref()
        .context("paired-device management is unavailable when launched with --mnemonic")?;
    session
        .catalog
        .remove_paired_host(profile, statement_account_id)?;
    session.responders.remove(statement_account_id);
    if let Err(error) = session
        .runtime
        .untrack_statement_renewal_account(statement_account_id)
        .await
    {
        tracing::warn!(
            reason = %error.reason,
            "paired device was removed, but its allowance renewal could not be removed"
        );
    }
    Ok(())
}

fn validate_session_clear(
    session: &SigningHostSession,
    target: &SessionClearTarget,
) -> Result<bool> {
    if session.mnemonic.is_some() {
        bail!("session clearing is unavailable when launched with --mnemonic");
    }
    session.catalog.validate_clear_target(target)?;
    Ok(match target {
        SessionClearTarget::All => true,
        SessionClearTarget::Named(name) => session
            .profile
            .as_ref()
            .is_some_and(|profile| profile.name == *name),
    })
}

fn session_clear_confirmation(
    session: &SigningHostSession,
    target: &SessionClearTarget,
    active: bool,
) -> (String, String) {
    let action = match target {
        SessionClearTarget::Named(name) => format!("Clear session {name}"),
        SessionClearTarget::All => {
            format!("Clear all sessions for {}", session.catalog.network_id())
        }
    };
    let scope = match target {
        SessionClearTarget::Named(_) => {
            "This permanently deletes its local signer keys, scripts, storage, and permissions"
        }
        SessionClearTarget::All => {
            "This permanently deletes every local session's signer keys, scripts, storage, and permissions"
        }
    };
    let mut detail = format!(
        "{scope} for {}. On-chain usernames are not removed.",
        session.catalog.network_id()
    );
    if active {
        detail.push_str(" The active session is included, so the signing host will stop.");
    }
    (action, detail)
}

fn session_clear_success(
    network_id: &str,
    target: &SessionClearTarget,
    stopped: bool,
) -> (String, String) {
    let title = match target {
        SessionClearTarget::Named(name) => format!("Session {name} cleared"),
        SessionClearTarget::All => format!("All sessions cleared for {network_id}"),
    };
    let detail = if stopped {
        "Signing host stopped. Restart it to create or select another session.".to_string()
    } else {
        format!(
            "Local signer keys, scripts, storage, and permissions were deleted for {network_id}."
        )
    };
    (title, detail)
}

fn clear_sessions_after_shutdown(
    catalog: &SessionCatalog,
    target: &SessionClearTarget,
) -> Result<()> {
    catalog.clear(target)?;
    let (title, detail) = session_clear_success(catalog.network_id(), target, true);
    terminal_ui::output_success(title, Some(detail));
    Ok(())
}

fn session_status_event(session: &SigningHostSession) -> SystemEvent {
    let name = session
        .profile
        .as_ref()
        .map_or("ephemeral", |profile| profile.name.as_str());
    let path = session.profile.as_ref().map_or_else(
        || "<none>".to_string(),
        |profile| profile.path.display().to_string(),
    );
    let user_id = session
        .signer
        .as_ref()
        .and_then(|signer| signer.lite_username.as_deref())
        .or(session.cached_user_id.as_deref())
        .unwrap_or_else(|| {
            if session.signer.is_some() {
                "<no assigned username>"
            } else {
                "<not provisioned>"
            }
        });
    SystemEvent::SessionStatus {
        name: name.to_string(),
        path,
        user_id: user_id.to_string(),
    }
}

fn session_status(session: &SigningHostSession) -> String {
    session_status_event(session).human()
}

fn session_list(session: &SigningHostSession) -> Result<String> {
    let current = session
        .profile
        .as_ref()
        .map_or("ephemeral", |profile| profile.name.as_str());
    let mut lines = vec!["Sessions".to_string()];
    for name in session.catalog.list()? {
        let marker = if name == current { "*" } else { " " };
        let path = session.catalog.profile(&name)?.path;
        lines.push(format!("{marker} {name}  {}", path.display()));
    }
    if lines.len() == 1 && session.profile.is_some() {
        lines.push("  <none>".to_string());
    }
    if session.profile.is_none() {
        lines.push("* ephemeral  <none>".to_string());
    }
    Ok(lines.join("\n"))
}

fn paired_device_label(host: &PairedHost) -> String {
    let metadata = host.metadata();
    let mut parts = Vec::new();
    if let Some(name) = &metadata.host_name {
        parts.push(name.clone());
    }
    if let Some(version) = &metadata.host_version {
        parts.push(version.clone());
    }
    let platform = match (&metadata.platform_type, &metadata.platform_version) {
        (Some(platform), Some(version)) => Some(format!("{platform} {version}")),
        (Some(platform), None) => Some(platform.clone()),
        (None, Some(version)) => Some(version.clone()),
        (None, None) => None,
    };
    if let Some(platform) = platform {
        parts.push(platform);
    }
    if parts.is_empty() {
        "<unknown device>".to_string()
    } else {
        parts.join(" · ")
    }
}

fn paired_device_list(session: &SigningHostSession) -> Result<String> {
    let profile = session
        .profile
        .as_ref()
        .context("paired-device management is unavailable when launched with --mnemonic")?;
    Ok(format_paired_device_list(
        &profile.name,
        session.catalog.paired_hosts(profile)?,
    ))
}

fn format_paired_device_list(session_name: &str, mut paired_hosts: Vec<PairedHost>) -> String {
    paired_hosts.sort_by_key(PairedHost::statement_account_id);
    if paired_hosts.is_empty() {
        return format!("No paired devices for session {session_name}");
    }
    let mut lines = vec![format!("Paired devices for session {session_name}")];
    lines.extend(paired_hosts.iter().map(|host| {
        format!(
            "0x{}  {}",
            hex::encode(host.statement_account_id()),
            paired_device_label(host)
        )
    }));
    lines.join("\n")
}

fn paired_device_remove_confirmation(
    session: &SigningHostSession,
    statement_account_id: &[u8; 32],
    force: bool,
) -> Result<(String, String)> {
    let profile = session
        .profile
        .as_ref()
        .context("paired-device management is unavailable when launched with --mnemonic")?;
    let host = find_paired_host(session, statement_account_id)?;
    let notification_failure = if force {
        "If notification fails, local removal still continues. The remote host may show stale connected state, but it cannot reach this responder after removal."
    } else {
        "If notification fails, nothing is removed."
    };
    Ok((
        format!("Remove paired device {}", paired_device_label(&host)),
        format!(
            "Statement account 0x{}. This notifies the remote host, then stops its responder and removes its saved pairing from session {}. {notification_failure} Other paired devices and the signing identity are unchanged. The remote host must pair again.",
            hex::encode(statement_account_id),
            profile.name
        ),
    ))
}

async fn switch_session(session: &mut SigningHostSession, name: String) -> Result<()> {
    if session.mnemonic.is_some() {
        bail!("session switching is unavailable when launched with --mnemonic");
    }
    sessions::validate_selectable_name(&name).map_err(anyhow::Error::msg)?;
    // A name that already provisioned a session was promoted to its Lite
    // username, so it selects that session rather than creating another.
    let name = session.catalog.resolve_session_name(&name)?;
    if session
        .profile
        .as_ref()
        .is_some_and(|profile| profile.name == name)
        && session.signer.is_some()
    {
        terminal_ui::output_event(session_status_event(session));
        return Ok(());
    }
    activate_session(session, name).await?;
    restore_paired_responders(session).await;
    Ok(())
}

async fn activate_session(session: &mut SigningHostSession, name: String) -> Result<()> {
    let provisional_profile = session.catalog.profile(&name)?;
    let account = selected_session_account(
        &session.catalog,
        &provisional_profile,
        session.default_account.as_deref(),
    )?;
    if is_lite_label(&name)
        && accounts::AccountStore::load(&provisional_profile.account_base_path)?
            .created_at(session.network.id, account.as_deref())?
            .is_none()
    {
        bail!("session {name:?} has no saved account; choose a username base to create one");
    }
    let lite_username_prefix = sessions::lite_username_prefix(&name);
    if !provisional_profile.is_provisioned()
        && lite_username_prefix
            .as_ref()
            .is_some_and(|prefix| prefix.len() < MIN_PERSON_LABEL_LEN)
    {
        bail!(
            "session name {name:?} must contain at least {MIN_PERSON_LABEL_LEN} lowercase ASCII letters to create an account; digits and separators do not count"
        );
    }
    let existed = session.catalog.exists(&name);
    let old_name = session
        .profile
        .as_ref()
        .map_or(DEFAULT_SESSION_NAME, |profile| profile.name.as_str())
        .to_string();
    if existed {
        terminal_ui::output_event(SystemEvent::SessionSwitching {
            from: old_name.clone(),
            to: name.clone(),
        });
    } else {
        terminal_ui::output_event(SystemEvent::SessionCreating { name: name.clone() });
    }

    // Resolve and provision the target completely while the old runtime keeps
    // serving. Only the final runtime replacement invalidates product sockets.
    let provisional_profile = session.catalog.ensure_profile(&name)?;
    if !provisional_profile.is_provisioned() {
        terminal_ui::output_event(SystemEvent::SigningHostProvisioning);
    }
    let signer = resolve_session_signer(ResolveSignerConfig {
        base_path: &provisional_profile.account_base_path,
        network: session.network,
        mnemonic: None,
        account,
        lite_username_prefix,
        reserved_username: session.reserved_username.clone(),
    })
    .await?;
    let (profile, promotion) = if let Some(user_id) = &signer.lite_username {
        let promotion = session
            .catalog
            .prepare_promotion(&provisional_profile, user_id)?;
        (promotion.profile().clone(), Some(promotion))
    } else {
        (provisional_profile, None)
    };
    let last_script = session.catalog.last_script(&profile)?;
    let (runtime, platform) = build_signing_runtime(
        session.network,
        profile.path.clone(),
        profile.product_storage_dir.clone(),
        session.platform.approval_policy(),
        session.ui.clone(),
        session.chat.clone(),
        session.pocket.clone(),
    )?;
    let available_sessions = session.catalog.list()?;

    if let Err(error) = runtime
        .activate_local_session_with_identity(signer.entropy.clone(), signer.lite_username.clone())
        .await
    {
        bail!("failed to activate session {name:?}: {}", error.reason);
    }
    if let (Some(user_id), Some(account_name)) = (&signer.lite_username, &signer.account_name) {
        session
            .catalog
            .store_signer_binding(&profile, user_id, account_name)?;
    }
    session.catalog.set_current(&profile.name)?;
    if let Some(promotion) = promotion {
        promotion.commit();
    }

    session.responders.stop_all();
    session.runtime_factory.replace(runtime.clone());
    session.runtime = runtime;
    session.platform = platform;
    session.cached_user_id = signer.lite_username.clone();
    session.signer = Some(signer);
    session.last_script = last_script;
    session.profile = Some(profile);
    if let Some(ui) = &session.ui {
        let current_name = session
            .profile
            .as_ref()
            .map(|profile| profile.name.clone())
            .unwrap_or(name);
        ui.session(current_name, available_sessions);
        if let Some(user_id) = &session.cached_user_id {
            ui.connection(user_id.clone());
        }
    }
    terminal_ui::output_event(session_status_event(session));
    Ok(())
}

/// Import an existing mnemonic as a durable session, using its existing
/// identity username when one exists and a key fingerprint otherwise.
/// Inspection and runtime activation happen before the secret is committed
/// locally, so the current session keeps serving on failure.
async fn import_mnemonic_session(
    session: &mut SigningHostSession,
    mnemonic: &crate::signing_shell::SecretMnemonic,
) -> Result<()> {
    if session.mnemonic.is_some() {
        bail!("session import is unavailable when launched with --mnemonic");
    }

    terminal_ui::update_activity(
        "signer",
        "Importing signer",
        Some("Checking identity and ring membership".to_string()),
        ActivityState::Running,
    );
    let imported =
        accounts::inspect_imported_signer(session.network, mnemonic.expose_secret()).await?;
    let username = imported.username().map(str::to_string);
    let session_name = imported.session_name().to_string();
    sessions::validate_selectable_name(&session_name).map_err(anyhow::Error::msg)?;

    let old_name = session
        .profile
        .as_ref()
        .map_or(DEFAULT_SESSION_NAME, |profile| profile.name.as_str())
        .to_string();
    if session.catalog.exists(&session_name) {
        terminal_ui::output_event(SystemEvent::SessionSwitching {
            from: old_name,
            to: session_name.clone(),
        });
    } else {
        terminal_ui::output_event(SystemEvent::SessionCreating {
            name: session_name.clone(),
        });
    }

    let profile = session.catalog.ensure_profile(&session_name)?;
    let last_script = session.catalog.last_script(&profile)?;
    let (runtime, platform) = build_signing_runtime(
        session.network,
        profile.path.clone(),
        profile.product_storage_dir.clone(),
        session.platform.approval_policy(),
        session.ui.clone(),
        session.chat.clone(),
        session.pocket.clone(),
    )?;
    runtime
        .activate_local_session_with_identity(imported.entropy().to_vec(), username.clone())
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to activate imported session {session_name:?}: {}",
                error.reason
            )
        })?;

    let signer = accounts::persist_imported_signer(
        &profile.account_base_path,
        session.network.id,
        &imported,
    )?;
    let account_name = signer
        .account_name
        .as_deref()
        .context("imported signer has no stored account name")?;
    if let Some(username) = &username {
        session
            .catalog
            .store_signer_binding(&profile, username, account_name)?;
    } else {
        session
            .catalog
            .store_account_binding(&profile, account_name)?;
    }
    session.catalog.set_current(&session_name)?;
    let available_sessions = session.catalog.list()?;

    session.responders.stop_all();
    session.runtime_factory.replace(runtime.clone());
    session.runtime = runtime;
    session.platform = platform;
    session.cached_user_id = username.clone();
    session.signer = Some(signer);
    session.last_script = last_script;
    session.profile = Some(profile);
    if let Some(ui) = &session.ui {
        ui.session(session_name.clone(), available_sessions);
        if let Some(username) = &username {
            ui.connection(username.clone());
        }
    }
    let detail = username.as_ref().map_or_else(
        || {
            "The account is connected by key; it has no assigned username on this network."
                .to_string()
        },
        |username| format!("Connected as identity user {username}."),
    );
    terminal_ui::output_success(format!("Imported session {session_name}"), Some(detail));
    let activity_detail = username.map_or_else(
        || "connected by account key (no assigned username)".to_string(),
        |username| format!("identity username {username}"),
    );
    terminal_ui::update_activity(
        "signer",
        "Imported signer",
        Some(activity_detail),
        ActivityState::Succeeded,
    );
    terminal_ui::output_event(session_status_event(session));
    restore_paired_responders(session).await;
    Ok(())
}

async fn pairing_interactive_loop(
    frame_url: String,
    product: Arc<frame_server::ProductSelection>,
    runtime: Arc<PairingHostRuntime>,
    storage: Arc<CliPlatform>,
    script_projects: PathBuf,
    mut ui: ActiveTerminalUi,
    log_controller: LogController,
) -> Result<()> {
    let mut pairing_state_path = storage
        .state_dir()
        .context("pairing host storage is not configured")?;
    let mut last_script = sessions::session_last_script(&pairing_state_path)?;
    loop {
        let Some(input) = ui.next_command().await? else {
            return Ok(());
        };
        ui.command(input.clone());
        let command = match parse_command(&input) {
            Ok(command) => command,
            Err(error) => {
                ui.error(error);
                continue;
            }
        };
        match command {
            ShellCommand::Help => ui.system(PAIRING_HELP_TEXT),
            ShellCommand::Clear => ui.clear(),
            ShellCommand::Copy => match ui.copy_transcript() {
                Ok(entries) => ui.event(SystemEvent::CopiedTranscript { entries }),
                Err(error) => ui.error(format!("failed to copy transcript: {error}")),
            },
            ShellCommand::Login => {
                let product_id = product.current();
                run_pairing_login(&runtime, &product_id, input, &mut ui).await?;
            }
            ShellCommand::Logout => match runtime.logout().await {
                Ok(()) => ui.success(
                    "Logged out",
                    Some(
                        "The next product login will generate new pairing keys and a fresh link."
                            .to_string(),
                    ),
                ),
                Err(error) => ui.error(format!("logout failed: {}", error.reason)),
            },
            ShellCommand::Log(level) => {
                if let Err(error) = log_controller.set(level) {
                    ui.error(format!("failed to set log level: {error}"));
                } else {
                    ui.set_log_level(level);
                    ui.event(SystemEvent::LogLevelChanged { level });
                }
            }
            ShellCommand::Product(ProductCommand::Current) => ui.system(product.current()),
            ShellCommand::Product(ProductCommand::Switch(product_id)) => {
                match product.select(product_id) {
                    Ok(true) => {
                        let product_id = product.current();
                        ui.set_product(product_id.clone());
                        ui.success(
                            format!("Product set to {product_id}"),
                            Some("Reconnect product clients to continue.".to_string()),
                        );
                    }
                    Ok(false) => ui.system(product.current()),
                    Err(error) => ui.error(error.to_string()),
                }
            }
            ShellCommand::Quit => return Ok(()),
            ShellCommand::Script(command) => {
                let current_state_path = storage
                    .state_dir()
                    .context("pairing host storage is not configured")?;
                if current_state_path != pairing_state_path {
                    pairing_state_path = current_state_path;
                    last_script = sessions::session_last_script(&pairing_state_path)?;
                }
                let script = select_interactive_script(
                    &command,
                    &script_projects,
                    Some(&pairing_state_path),
                    &mut last_script,
                    &mut ui,
                )
                .await;
                match script {
                    Ok(Some(script)) => {
                        let product_id = product.current();
                        run_pairing_script(&frame_url, &product_id, &script, input, &mut ui)
                            .await?;
                    }
                    Ok(None) => {}
                    Err(error) => ui.error(error.to_string()),
                }
            }
            ShellCommand::Pair(_)
            | ShellCommand::Devices(_)
            | ShellCommand::Approval(_)
            | ShellCommand::Session(_)
            | ShellCommand::Renew => {
                ui.error("command is only available on the signing host");
            }
        }
    }
}

async fn run_pairing_login(
    runtime: &PairingHostRuntime,
    product_id: &str,
    label: String,
    ui: &mut ActiveTerminalUi,
) -> Result<()> {
    let checkpoint = ui.activity_checkpoint();
    match ui
        .drive_pairing_login(label, runtime.login(product_id))
        .await?
    {
        DriveResult::Complete(Ok(truapi::v01::HostRequestLoginResponse::Success)) => {}
        DriveResult::Complete(Ok(truapi::v01::HostRequestLoginResponse::AlreadyConnected)) => {
            ui.success("Already logged in", None);
        }
        DriveResult::Complete(Ok(truapi::v01::HostRequestLoginResponse::Rejected)) => {
            ui.finish_activities_since(checkpoint, ActivityState::Cancelled, "Login was rejected");
            ui.error("login rejected");
        }
        DriveResult::Complete(Err(error)) => {
            ui.finish_activities_since(
                checkpoint,
                ActivityState::Failed,
                "Login stopped after an error",
            );
            ui.error(format!("login failed: {}", error.reason));
        }
        DriveResult::Cancelled => {
            runtime.cancel_pairing();
            ui.finish_activities_since(checkpoint, ActivityState::Cancelled, "Login cancelled");
            ui.error("login cancelled");
        }
    }
    Ok(())
}

async fn run_pairing_script(
    frame_url: &str,
    product_id: &str,
    script: &std::path::Path,
    label: String,
    ui: &mut ActiveTerminalUi,
) -> Result<()> {
    let activity_checkpoint = ui.activity_checkpoint();
    let handle = ui.handle();
    let operation = async {
        let status = script_runner::run_captured(
            frame_url,
            product_id,
            script,
            handle,
            script_runner::ScriptHostRole::PairingHost,
        )
        .await?;
        terminal_ui::output_event(SystemEvent::ScriptExit {
            code: status.code().unwrap_or(1),
        });
        Ok::<(), anyhow::Error>(())
    };
    match ui.drive(label, operation).await? {
        DriveResult::Complete(Ok(())) => {}
        DriveResult::Complete(Err(error)) => {
            ui.finish_activities_since(
                activity_checkpoint,
                ActivityState::Failed,
                "Stopped after an error",
            );
            ui.error(error.to_string());
        }
        DriveResult::Cancelled => {
            ui.finish_activities_since(activity_checkpoint, ActivityState::Cancelled, "Cancelled");
            ui.error("command cancelled");
        }
    }
    Ok(())
}

async fn signing_interactive_loop(
    session: &mut SigningHostSession,
    frame_url: String,
    product: Arc<frame_server::ProductSelection>,
    initial_command: Option<(String, ShellCommand)>,
    mut ui: ActiveTerminalUi,
    log_controller: LogController,
) -> Result<Option<SessionClearTarget>> {
    if let Some((input, command)) = initial_command {
        ui.command(input.clone());
        let product_id = product.current();
        run_interactive_operation(session, &frame_url, &product_id, command, input, &mut ui)
            .await?;
    }

    loop {
        let Some(input) = ui.next_command().await? else {
            return Ok(None);
        };
        ui.command(input.clone());
        let command = match parse_command(&input) {
            Ok(command) => command,
            Err(error) => {
                ui.error(error);
                continue;
            }
        };
        match command {
            ShellCommand::Help => ui.system(HELP_TEXT),
            ShellCommand::Clear => ui.clear(),
            ShellCommand::Copy => match ui.copy_transcript() {
                Ok(entries) => ui.event(SystemEvent::CopiedTranscript { entries }),
                Err(error) => ui.error(format!("failed to copy transcript: {error}")),
            },
            ShellCommand::Log(level) => {
                if let Err(error) = log_controller.set(level) {
                    ui.error(format!("failed to set log level: {error}"));
                } else {
                    ui.set_log_level(level);
                    ui.event(SystemEvent::LogLevelChanged { level });
                }
            }
            ShellCommand::Approval(ApprovalCommand::Current) => {
                let mode = match session.platform.approval_policy() {
                    ApprovalPolicy::Prompt => "manual",
                    ApprovalPolicy::AutoAccept => "automatic",
                };
                ui.system(format!("Approval mode: {mode}"));
            }
            ShellCommand::Approval(ApprovalCommand::Manual) => {
                session.platform.set_approval_policy(ApprovalPolicy::Prompt);
                ui.success(
                    "Approval mode set to manual",
                    Some("Future product confirmations will require approval.".to_string()),
                );
            }
            ShellCommand::Approval(ApprovalCommand::Automatic) => {
                session
                    .platform
                    .set_approval_policy(ApprovalPolicy::AutoAccept);
                ui.success(
                    "Approval mode set to automatic",
                    Some(
                        "Future product confirmations will be approved automatically.".to_string(),
                    ),
                );
            }
            ShellCommand::Product(ProductCommand::Current) => ui.system(product.current()),
            ShellCommand::Product(ProductCommand::Switch(product_id)) => {
                match product.select(product_id) {
                    Ok(true) => {
                        let product_id = product.current();
                        ui.set_product(product_id.clone());
                        ui.success(
                            format!("Product set to {product_id}"),
                            Some("Reconnect product clients to continue.".to_string()),
                        );
                    }
                    Ok(false) => ui.system(product.current()),
                    Err(error) => ui.error(error.to_string()),
                }
            }
            ShellCommand::Session(SessionCommand::Current) => {
                ui.event(session_status_event(session));
                if session.profile.is_some() && session.signer.is_none() {
                    ui.event(SystemEvent::SigningHostNeedsSession);
                }
            }
            ShellCommand::Session(SessionCommand::List) => match session_list(session) {
                Ok(sessions) => ui.system(sessions),
                Err(error) => ui.error(format!("failed to list sessions: {error}")),
            },
            ShellCommand::Devices(DeviceCommand::List) => match paired_device_list(session) {
                Ok(devices) => ui.system(devices),
                Err(error) => ui.error(format!("failed to list paired devices: {error}")),
            },
            ShellCommand::Devices(DeviceCommand::Remove {
                statement_account_id,
                force,
            }) => {
                let (action, detail) = match paired_device_remove_confirmation(
                    session,
                    &statement_account_id,
                    force,
                ) {
                    Ok(confirmation) => confirmation,
                    Err(error) => {
                        ui.error(error.to_string());
                        continue;
                    }
                };
                let handle = ui.handle();
                let approved = match ui
                    .drive(input.clone(), handle.confirm(action, detail))
                    .await?
                {
                    DriveResult::Complete(approved) => approved,
                    DriveResult::Cancelled => false,
                };
                if !approved {
                    ui.system("Paired-device removal cancelled");
                    continue;
                }
                match ui
                    .drive(
                        input,
                        disconnect_and_remove_paired_host(session, &statement_account_id, force),
                    )
                    .await?
                {
                    DriveResult::Complete(Ok(removal)) => {
                        if let Some(reason) = removal.notification_failure {
                            let (title, detail) = forced_removal_warning(&reason);
                            ui.warning(title, Some(detail));
                        }
                        ui.success(
                            "Paired device removed",
                            Some(format!(
                                "{}\nStatement account 0x{}",
                                paired_device_label(&removal.paired_host),
                                hex::encode(statement_account_id)
                            )),
                        );
                    }
                    DriveResult::Complete(Err(error)) => ui.error(error.to_string()),
                    DriveResult::Cancelled => ui.system("Paired-device removal cancelled"),
                }
            }
            ShellCommand::Session(SessionCommand::Clear(target)) => {
                let active = match validate_session_clear(session, &target) {
                    Ok(active) => active,
                    Err(error) => {
                        ui.error(error.to_string());
                        continue;
                    }
                };
                let (action, detail) = session_clear_confirmation(session, &target, active);
                let handle = ui.handle();
                let approved = match ui
                    .drive(input.clone(), handle.confirm(action, detail))
                    .await?
                {
                    DriveResult::Complete(approved) => approved,
                    DriveResult::Cancelled => false,
                };
                if !approved {
                    ui.system("Session clear cancelled");
                    continue;
                }
                if active || target == SessionClearTarget::All {
                    session.responders.stop_all();
                    session.runtime_factory.reset_connections();
                    tokio::task::yield_now().await;
                    return Ok(Some(target));
                }
                match session.catalog.clear(&target) {
                    Ok(_) => {
                        let (title, detail) =
                            session_clear_success(session.catalog.network_id(), &target, false);
                        ui.success(title, Some(detail));
                        let current = session
                            .profile
                            .as_ref()
                            .map_or(DEFAULT_SESSION_NAME, |profile| profile.name.as_str());
                        ui.handle().session(current, session.catalog.list()?);
                    }
                    Err(error) => ui.error(error.to_string()),
                }
            }
            ShellCommand::Quit => return Ok(None),
            ShellCommand::Pair(PairCommand::Scan) => {
                let input = match ui.read_pairing_image().await? {
                    DriveResult::Complete(input) => input,
                    DriveResult::Cancelled => continue,
                };
                run_interactive_pairing_image(session, input, &mut ui).await?;
            }
            ShellCommand::Script(command) => match select_interactive_script(
                &command,
                &session.script_projects,
                session
                    .profile
                    .as_ref()
                    .map(|profile| profile.path.as_path()),
                &mut session.last_script,
                &mut ui,
            )
            .await
            {
                Ok(Some(script)) => {
                    let product_id = product.current();
                    run_interactive_operation(
                        session,
                        &frame_url,
                        &product_id,
                        ShellCommand::Script(ScriptCommand::Run(Some(script))),
                        input,
                        &mut ui,
                    )
                    .await?;
                }
                Ok(None) => {}
                Err(error) => ui.error(error.to_string()),
            },
            command => {
                let product_id = product.current();
                run_interactive_operation(
                    session,
                    &frame_url,
                    &product_id,
                    command,
                    input,
                    &mut ui,
                )
                .await?;
            }
        }
    }
}

async fn run_interactive_operation(
    session: &mut SigningHostSession,
    frame_url: &str,
    product_id: &str,
    command: ShellCommand,
    label: String,
    ui: &mut ActiveTerminalUi,
) -> Result<()> {
    let activity_checkpoint = ui.activity_checkpoint();
    let handle = ui.handle();
    let operation = execute_interactive_operation(session, frame_url, product_id, command, handle);
    match ui.drive(label, operation).await? {
        DriveResult::Complete(Ok(())) => {}
        DriveResult::Complete(Err(error)) => {
            ui.finish_activities_since(
                activity_checkpoint,
                ActivityState::Failed,
                "Stopped after an error",
            );
            ui.error_with_causes(&error);
        }
        DriveResult::Cancelled => {
            ui.finish_activities_since(activity_checkpoint, ActivityState::Cancelled, "Cancelled");
            ui.error("command cancelled");
        }
    }
    Ok(())
}

async fn run_interactive_pairing_image(
    session: &mut SigningHostSession,
    input: PairingImageInput,
    ui: &mut ActiveTerminalUi,
) -> Result<()> {
    let activity_checkpoint = ui.activity_checkpoint();
    let operation = async {
        let deeplink = tokio::task::spawn_blocking(move || match input {
            PairingImageInput::Pixels(image) => qr_scanner::decode(&image),
            PairingImageInput::Path(path) => qr_scanner::decode_path(&path),
        })
        .await
        .context("join QR image decoder")??;
        start_deeplink_responder(session, deeplink).await
    };
    match ui.drive("/pair", operation).await? {
        DriveResult::Complete(Ok(())) => {}
        DriveResult::Complete(Err(error)) => {
            ui.finish_activities_since(
                activity_checkpoint,
                ActivityState::Failed,
                "Stopped after an error",
            );
            ui.error_with_causes(&error);
        }
        DriveResult::Cancelled => {
            ui.finish_activities_since(activity_checkpoint, ActivityState::Cancelled, "Cancelled");
            ui.error("command cancelled");
        }
    }
    Ok(())
}

async fn execute_interactive_operation(
    session: &mut SigningHostSession,
    frame_url: &str,
    product_id: &str,
    command: ShellCommand,
    ui: UiHandle,
) -> Result<()> {
    match command {
        ShellCommand::Pair(PairCommand::Deeplink(deeplink)) => {
            start_deeplink_responder(session, deeplink).await?
        }
        ShellCommand::Pair(PairCommand::Image(path)) => {
            let deeplink = tokio::task::spawn_blocking(move || qr_scanner::decode_path(&path))
                .await
                .context("join QR image decoder")??;
            start_deeplink_responder(session, deeplink).await?;
        }
        ShellCommand::Pair(PairCommand::Scan) => {
            bail!("clipboard image paste must be handled by the terminal UI")
        }
        ShellCommand::Script(ScriptCommand::Run(Some(script))) => {
            let session_path = session.profile.as_ref().map(|profile| profile.path.clone());
            let script =
                remember_script(session_path.as_deref(), &mut session.last_script, script)?;
            ensure_signer(session).await?;
            let status = script_runner::run_captured(
                frame_url,
                product_id,
                &script,
                ui,
                script_runner::ScriptHostRole::SigningHost,
            )
            .await?;
            terminal_ui::output_event(SystemEvent::ScriptExit {
                code: status.code().unwrap_or(1),
            });
        }
        ShellCommand::Script(_) => bail!("script selection must be handled by the terminal UI"),
        ShellCommand::Session(SessionCommand::Switch(name)) => {
            switch_session(session, name).await?;
        }
        ShellCommand::Session(SessionCommand::ImportMnemonic(mnemonic)) => {
            import_mnemonic_session(session, &mnemonic).await?;
        }
        ShellCommand::Renew => run_renew(session).await?,
        ShellCommand::Login => bail!("/login is only available on the pairing host"),
        ShellCommand::Logout => bail!("/logout is only available on the pairing host"),
        ShellCommand::Product(_) | ShellCommand::Devices(_) | ShellCommand::Approval(_) => {
            bail!("command must be handled by the terminal UI")
        }
        ShellCommand::Help
        | ShellCommand::Clear
        | ShellCommand::Copy
        | ShellCommand::Log(_)
        | ShellCommand::Session(
            SessionCommand::Current | SessionCommand::List | SessionCommand::Clear(_),
        )
        | ShellCommand::Quit => {
            bail!("command must be handled by the terminal UI")
        }
    }
    Ok(())
}

async fn execute_non_interactive_command(
    session: &mut SigningHostSession,
    frame_url: &str,
    product: &frame_server::ProductSelection,
    command: ShellCommand,
    log_controller: &LogController,
) -> Result<Option<SessionClearTarget>> {
    match command {
        ShellCommand::Pair(PairCommand::Deeplink(deeplink)) => {
            respond_to_deeplink(session, deeplink).await?
        }
        ShellCommand::Pair(PairCommand::Image(path)) => {
            let deeplink = tokio::task::spawn_blocking(move || qr_scanner::decode_path(&path))
                .await
                .context("join QR image decoder")??;
            respond_to_deeplink(session, deeplink).await?
        }
        ShellCommand::Pair(PairCommand::Scan) => bail!(
            "clipboard image paste needs an interactive signing host; use /pair <image-path> or /pair <polkadotapp://pair?...>"
        ),
        ShellCommand::Script(command) => {
            if command.edits() && !terminal_ui::is_interactive_terminal() {
                bail!(
                    "/script without a path requires an interactive terminal; use /script --run or /script <path>"
                );
            }
            let script =
                select_script(&command, &session.script_projects, &mut session.last_script)?;
            let session_path = session
                .profile
                .as_ref()
                .map(|profile| profile.path.as_path());
            let script = remember_script(session_path, &mut session.last_script, script)?;
            let script = if command.edits() {
                edit_script_plain(script).await?
            } else {
                script
            };
            if !command.runs() {
                return Ok(None);
            }
            ensure_signer(session).await?;
            let product_id = product.current();
            let status = script_runner::run(
                frame_url,
                &product_id,
                &script,
                script_runner::ScriptHostRole::SigningHost,
            )
            .await?;
            let code = status.code().unwrap_or(1);
            terminal_ui::output_event(SystemEvent::ScriptExit { code });
            if !status.success() {
                bail!("script exited with code {code}");
            }
        }
        ShellCommand::Help => println!("{HELP_TEXT}"),
        ShellCommand::Clear | ShellCommand::Quit => {}
        ShellCommand::Copy => bail!("/copy is only available in the terminal UI"),
        ShellCommand::Approval(_) => {
            bail!("/approval is only available in the terminal UI")
        }
        ShellCommand::Login => bail!("/login is only available on the pairing host"),
        ShellCommand::Logout => bail!("/logout is only available on the pairing host"),
        ShellCommand::Log(level) => {
            log_controller.set(level)?;
            terminal_ui::output_event(SystemEvent::LogLevelChanged { level });
        }
        ShellCommand::Product(ProductCommand::Current) => {
            println!("{}", product.current());
        }
        ShellCommand::Product(ProductCommand::Switch(product_id)) => {
            if product.select(product_id)? {
                terminal_ui::output_success(
                    format!("Product set to {}", product.current()),
                    Some("Reconnect product clients to continue.".to_string()),
                );
            } else {
                println!("{}", product.current());
            }
        }
        ShellCommand::Session(SessionCommand::Current) => {
            println!("{}", session_status(session));
            if session.profile.is_some() && session.signer.is_none() {
                terminal_ui::output_event(SystemEvent::SigningHostNeedsSession);
            }
        }
        ShellCommand::Session(SessionCommand::List) => {
            println!("{}", session_list(session)?);
        }
        ShellCommand::Devices(DeviceCommand::List) => {
            println!("{}", paired_device_list(session)?);
        }
        ShellCommand::Devices(DeviceCommand::Remove {
            statement_account_id,
            force,
        }) => {
            let profile_name = session
                .profile
                .as_ref()
                .context("paired-device management is unavailable when launched with --mnemonic")?
                .name
                .clone();
            let removal =
                disconnect_and_remove_paired_host(session, &statement_account_id, force).await?;
            if let Some(reason) = removal.notification_failure {
                let (title, detail) = forced_removal_warning(&reason);
                terminal_ui::output_warning(title, Some(detail));
            }
            println!(
                "Removed paired device 0x{} from session {}",
                hex::encode(statement_account_id),
                profile_name
            );
        }
        ShellCommand::Session(SessionCommand::Switch(name)) => {
            switch_session(session, name).await?;
        }
        ShellCommand::Session(SessionCommand::ImportMnemonic(mnemonic)) => {
            import_mnemonic_session(session, &mnemonic).await?;
        }
        ShellCommand::Session(SessionCommand::Clear(target)) => {
            validate_session_clear(session, &target)?;
            session.responders.stop_all();
            session.runtime_factory.reset_connections();
            tokio::task::yield_now().await;
            return Ok(Some(target));
        }
        ShellCommand::Renew => run_renew(session).await?,
    }
    Ok(None)
}

fn select_script_to_edit(
    scratch_script_directory: &std::path::Path,
    last_script: &mut Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(script) = last_script
        .as_ref()
        .map(|script| script_project::remembered_script(script))
        .transpose()?
        .flatten()
    {
        return Ok(script);
    }
    let script = script_project::create(scratch_script_directory, None)?;
    *last_script = Some(script.clone());
    Ok(script)
}

fn select_script(
    command: &ScriptCommand,
    projects: &Path,
    last_script: &mut Option<PathBuf>,
) -> Result<PathBuf> {
    match command {
        ScriptCommand::Edit | ScriptCommand::EditOnly => {
            select_script_to_edit(projects, last_script)
        }
        ScriptCommand::New(directory) => script_project::create(projects, directory.as_deref()),
        ScriptCommand::Run(Some(script)) => Ok(script.clone()),
        ScriptCommand::Run(None) => last_script
            .as_ref()
            .map(|script| script_project::remembered_script(script))
            .transpose()?
            .flatten()
            .context(
                "no script selected; use /script to create one or /script <path> to select one",
            ),
    }
}

fn remember_script(
    session_path: Option<&std::path::Path>,
    last_script: &mut Option<PathBuf>,
    script: PathBuf,
) -> Result<PathBuf> {
    let script = if script.is_absolute() {
        script
    } else {
        std::env::current_dir()
            .context("resolve current directory for script")?
            .join(script)
    };
    if let Some(session_path) = session_path {
        sessions::store_session_last_script(session_path, &script)?;
    }
    *last_script = Some(script.clone());
    Ok(script)
}

async fn select_interactive_script(
    command: &ScriptCommand,
    projects: &Path,
    session_path: Option<&Path>,
    last_script: &mut Option<PathBuf>,
    ui: &mut ActiveTerminalUi,
) -> Result<Option<PathBuf>> {
    let script = select_script(command, projects, last_script)?;
    let script = remember_script(session_path, last_script, script)?;
    let script = if command.edits() {
        edit_script_in(script, ui).await?
    } else {
        script
    };
    Ok(command.runs().then_some(script))
}

async fn edit_script_in(script: PathBuf, ui: &mut ActiveTerminalUi) -> Result<PathBuf> {
    match ui
        .drive(
            "Preparing script",
            script_project::prepare(&script, Some(ui.handle())),
        )
        .await?
    {
        DriveResult::Complete(result) => result?,
        DriveResult::Cancelled => bail!(
            "script setup cancelled; project retained at {}",
            script.display()
        ),
    }
    ui.system(format!("Opening {} in your editor", script.display()));
    ui.suspend()?;
    let edit_result = script_runner::edit(&script).await;
    let resume_result = ui.resume();
    if let Err(error) = resume_result {
        return Err(error).context("restore terminal UI after editor");
    }
    let status = edit_result?;
    if !status.success() {
        bail!(
            "editor exited with {}; script retained at {}",
            status.code().unwrap_or(1),
            script.display()
        );
    }
    ui.success("Script saved", Some(script.display().to_string()));
    Ok(script)
}

async fn edit_script_plain(script: PathBuf) -> Result<PathBuf> {
    script_project::prepare(&script, None).await?;
    eprintln!("EDITING_SCRIPT {}", script.display());
    let status = script_runner::edit(&script).await?;
    if !status.success() {
        bail!(
            "editor exited with {}; script retained at {}",
            status.code().unwrap_or(1),
            script.display()
        );
    }
    eprintln!("SAVED_SCRIPT {}", script.display());
    Ok(script)
}

fn default_base_path() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        return PathBuf::from(path).join("truapi-host");
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local/state/truapi-host");
    }
    PathBuf::from(".truapi-host")
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use crate::frame_server::ProductRuntimeFactory;
    use parity_scale_codec::Encode;

    const SESSION_MNEMONIC: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    async fn unfinished_session(base_path: &Path, name: &str) -> Result<SigningHostSession> {
        let network = Network::default().config();
        let catalog = SessionCatalog::new(base_path.to_path_buf(), network.id)?;
        catalog.set_current(name)?;
        start_signing_host(
            &SigningHostArgs::default(),
            catalog,
            name.to_string(),
            network,
            None,
        )
        .await
    }

    fn finish_session_account(session: &SigningHostSession) -> Result<Vec<u8>> {
        let profile = session.profile.as_ref().unwrap();
        let accounts = serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "accounts": [{
                "name": "auto-1",
                "network": session.network.id,
                "mnemonic": SESSION_MNEMONIC,
                "lite_username": "pending.01",
                "public_key_hex": "0x00",
                "address": "test",
                "created_at_unix": 1,
                "attested": true
            }]
        }))?;
        std::fs::write(profile.account_base_path.join("accounts.json"), &accounts)?;
        Ok(accounts)
    }

    #[tokio::test]
    async fn session_selection_retries_the_active_unfinished_profile() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut session = unfinished_session(temporary.path(), "pending").await?;
        session.network.identity_backend_base = "invalid-backend-url";
        let runtime = session.runtime.clone();

        let error = switch_session(&mut session, "pending".to_string())
            .await
            .expect_err("an unfinished active session must attempt provisioning");

        assert_eq!(
            (
                error.to_string(),
                Arc::ptr_eq(&runtime, &session.runtime),
                session.catalog.current_session()?,
                session.runtime.has_active_session(),
            ),
            (
                "check lite username \"pending\" availability".to_string(),
                true,
                CurrentSession::Pointed("pending".to_string()),
                false,
            )
        );
        Ok(())
    }

    #[tokio::test]
    async fn session_selection_activates_a_finished_account_without_reprovisioning() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut session = unfinished_session(temporary.path(), "pending").await?;
        let accounts = finish_session_account(&session)?;
        session.network.identity_backend_base = "invalid-backend-url";
        let mut resets = session.runtime_factory.connection_reset().unwrap();
        let expected_key =
            truapi::host_logic::product_account::derive_root_keypair_from_entropy(&[0; 16])
                .unwrap()
                .public
                .to_bytes();

        switch_session(&mut session, "pending".to_string()).await?;

        assert_eq!(
            (
                session.runtime.has_active_session(),
                session.runtime.statement_renewal_owner_key().ok(),
                session.signer.as_ref().map(|signer| signer.entropy.clone()),
                session
                    .profile
                    .as_ref()
                    .map(|profile| profile.name.as_str()),
                session.catalog.current_session()?,
                *resets.borrow_and_update(),
                std::fs::read(
                    session
                        .profile
                        .as_ref()
                        .unwrap()
                        .account_base_path
                        .join("accounts.json")
                )?,
            ),
            (
                true,
                Some(expected_key),
                Some(vec![0; 16]),
                Some("pending.01"),
                CurrentSession::Pointed("pending.01".to_string()),
                1,
                accounts,
            )
        );
        let runtime = session.runtime.clone();
        switch_session(&mut session, "pending".to_string()).await?;
        assert_eq!(
            (
                Arc::ptr_eq(&runtime, &session.runtime),
                resets.has_changed()?
            ),
            (true, false)
        );
        Ok(())
    }

    #[tokio::test]
    async fn session_selection_preserves_the_active_profile_when_promotion_cannot_commit()
    -> Result<()> {
        for (name, slash_command) in [
            (DEFAULT_SESSION_NAME, false),
            ("pending", true),
            ("pending", false),
        ] {
            let temporary = tempfile::tempdir()?;
            let mut session = unfinished_session(temporary.path(), name).await?;
            let profile = session.profile.clone().unwrap();
            let accounts = finish_session_account(&session)?;
            let contents = [
                (
                    profile.path.join("session.json"),
                    br#"{"version":1,"last_script":"saved.ts"}"#.to_vec(),
                ),
                (
                    profile.path.join("core-storage.json"),
                    br#"{"values":{}}"#.to_vec(),
                ),
                (
                    profile.path.join("scripts/saved.ts"),
                    b"console.log('saved');".to_vec(),
                ),
                (
                    profile.product_storage_dir.join("saved"),
                    b"product data".to_vec(),
                ),
            ];
            for (path, bytes) in &contents {
                std::fs::create_dir_all(path.parent().unwrap())?;
                std::fs::write(path, bytes)?;
            }
            let role = session.catalog.profile(DEFAULT_SESSION_NAME)?;
            let blocked_pointer = role
                .path
                .join(format!(".current-session.{}.tmp", std::process::id()));
            std::fs::create_dir(&blocked_pointer)?;
            let runtime = session.runtime.clone();
            let resets = session.runtime_factory.connection_reset().unwrap();

            let result = if slash_command {
                switch_session(&mut session, "pending".to_string()).await
            } else {
                ensure_signer(&mut session).await
            };
            let error =
                result.expect_err("failed persistence must leave the active profile usable");

            assert!(
                error.to_string().contains("write current session"),
                "{error:#}"
            );
            assert_eq!(
                (
                    Arc::ptr_eq(&runtime, &session.runtime),
                    session.profile.as_ref(),
                    session.catalog.current_session()?,
                    resets.has_changed()?,
                    std::fs::read(profile.account_base_path.join("accounts.json"))?,
                ),
                (
                    true,
                    Some(&profile),
                    CurrentSession::Pointed(name.to_string()),
                    false,
                    accounts.clone(),
                )
            );
            for (path, bytes) in &contents {
                assert_eq!(
                    std::fs::read(path)
                        .with_context(|| format!("read restored {}", path.display()))?,
                    *bytes,
                    "{}",
                    path.display()
                );
            }

            std::fs::remove_dir(blocked_pointer)?;
            if slash_command {
                switch_session(&mut session, "pending".to_string()).await?;
            } else {
                ensure_signer(&mut session).await?;
            }
            assert_eq!(
                (
                    session.runtime.has_active_session(),
                    session.signer.as_ref().map(|signer| signer.entropy.clone()),
                    std::fs::read(
                        session
                            .profile
                            .as_ref()
                            .unwrap()
                            .account_base_path
                            .join("accounts.json")
                    )?,
                ),
                (true, Some(vec![0; 16]), accounts)
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn session_selection_failure_keeps_the_working_identity_and_responders() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut session = unfinished_session(temporary.path(), "pending").await?;
        finish_session_account(&session)?;
        switch_session(&mut session, "pending".to_string()).await?;
        let runtime = session.runtime.clone();
        let owner = runtime.statement_renewal_owner_key().unwrap();
        let resets = session.runtime_factory.connection_reset().unwrap();
        let responder = tokio::spawn(std::future::pending::<()>());
        let responder_abort = responder.abort_handle();
        session.responders.insert([7; 32], responder);
        session.network.identity_backend_base = "invalid-backend-url";

        session.catalog.ensure_profile("workbench.99")?;
        for name in ["foo", "carol", "workbench.99", "another"] {
            let expected_error = match name {
                "foo" | "carol" => format!(
                    "session name {name:?} must contain at least 6 lowercase ASCII letters to create an account; digits and separators do not count"
                ),
                "workbench.99" => format!(
                    "session {name:?} has no saved account; choose a username base to create one"
                ),
                _ => format!("check lite username {name:?} availability"),
            };
            let error = switch_session(&mut session, name.to_string())
                .await
                .expect_err("failed selection must preserve the active signer");

            assert_eq!(
                (
                    error.to_string(),
                    Arc::ptr_eq(&runtime, &session.runtime),
                    session.runtime.statement_renewal_owner_key().ok(),
                    session.catalog.current_session()?,
                    resets.has_changed()?,
                    session.responders.tasks.keys().copied().collect::<Vec<_>>(),
                    responder_abort.is_finished(),
                    session.catalog.exists(name),
                ),
                (
                    expected_error,
                    true,
                    Some(owner),
                    CurrentSession::Pointed("pending.01".to_string()),
                    false,
                    vec![[7; 32]],
                    false,
                    matches!(name, "workbench.99" | "another"),
                )
            );
        }
        Ok(())
    }

    #[tokio::test]
    async fn the_signing_runtime_keeps_its_core_database_in_the_profile_directory() {
        // Each profile keeps its own durable state, so switching users never
        // mixes two ledgers.
        let profile = tempfile::tempdir().unwrap();
        let (runtime, _platform) = build_signing_runtime(
            crate::network::Network::PaseoNextV2.config(),
            profile.path().to_path_buf(),
            profile.path().join("storage"),
            ApprovalPolicy::AutoAccept,
            None,
            None,
            None,
        )
        .unwrap();

        let status = runtime.core_database_status().await.unwrap();

        let file = profile
            .path()
            .canonicalize()
            .unwrap()
            .join(truapi::store::CORE_DB_FILE);
        assert_eq!(
            status.path,
            Some(file.to_string_lossy().into_owned()),
            "the core database lives in the profile directory"
        );
    }

    #[test]
    fn pairing_deeplink_becomes_a_public_persistable_host_record() {
        use truapi::host_logic::sso::pairing::{
            VersionedHandshakeProposal,
            v2::{Device, MetadataEntry, MetadataKey, Proposal},
        };

        let proposal = VersionedHandshakeProposal::V2(Proposal {
            device: Device {
                statement_account_id: [1; 32],
                encryption_public_key: [2; 32],
            },
            metadata: vec![
                MetadataEntry(
                    MetadataKey::HostName,
                    " Desk\u{202e}top\u{2066}\nHost\u{061c} ".to_string(),
                ),
                MetadataEntry(MetadataKey::HostVersion, "1.2.3".to_string()),
            ],
        });
        let deeplink = format!(
            "polkadotapp://pair?handshake={}",
            hex::encode(proposal.encode())
        );

        assert_eq!(
            paired_host_from_deeplink(&deeplink).unwrap(),
            PairedHost::new(
                [1; 32],
                [2; 32],
                PairedHostMetadata {
                    host_name: Some("DesktopHost".to_string()),
                    host_version: Some("1.2.3".to_string()),
                    ..PairedHostMetadata::default()
                }
            )
        );
    }

    #[tokio::test]
    async fn responder_manager_keeps_unrelated_peers_and_replaces_only_the_same_peer() {
        let mut manager = ResponderManager::default();
        let first = tokio::spawn(std::future::pending::<()>());
        let first_abort = first.abort_handle();
        let second = tokio::spawn(std::future::pending::<()>());
        let second_abort = second.abort_handle();
        manager.insert([1; 32], first);
        manager.insert([2; 32], second);

        let replacement = tokio::spawn(std::future::pending::<()>());
        manager.insert([1; 32], replacement);
        tokio::task::yield_now().await;

        assert_eq!(
            (
                manager
                    .tasks
                    .keys()
                    .copied()
                    .collect::<std::collections::HashSet<_>>(),
                first_abort.is_finished(),
                second_abort.is_finished(),
            ),
            ([[1; 32], [2; 32]].into_iter().collect(), true, false,)
        );
    }

    #[tokio::test]
    async fn responder_manager_removes_only_the_selected_peer() {
        let mut manager = ResponderManager::default();
        let first = tokio::spawn(std::future::pending::<()>());
        let first_abort = first.abort_handle();
        let second = tokio::spawn(std::future::pending::<()>());
        let second_abort = second.abort_handle();
        manager.insert([1; 32], first);
        manager.insert([2; 32], second);

        assert!(manager.remove(&[1; 32]));
        tokio::task::yield_now().await;

        assert_eq!(
            (
                manager.tasks.keys().copied().collect::<Vec<_>>(),
                first_abort.is_finished(),
                second_abort.is_finished(),
            ),
            (vec![[2; 32]], true, false)
        );
    }

    #[test]
    fn paired_device_list_is_sorted_and_includes_available_display_metadata() {
        let named = PairedHost::new(
            [2; 32],
            [22; 32],
            PairedHostMetadata {
                host_name: Some("Desktop".to_string()),
                host_version: Some("1.2.3".to_string()),
                host_icon: None,
                platform_type: Some("macOS".to_string()),
                platform_version: Some("26.1".to_string()),
            },
        );
        let unknown = PairedHost::new([1; 32], [11; 32], PairedHostMetadata::default());

        assert_eq!(
            format_paired_device_list("alice.01", vec![named, unknown]),
            format!(
                "Paired devices for session alice.01\n0x{}  <unknown device>\n0x{}  Desktop · 1.2.3 · macOS 26.1",
                hex::encode([1; 32]),
                hex::encode([2; 32])
            )
        );
    }

    #[test]
    fn paired_device_list_reports_an_empty_session() {
        assert_eq!(
            format_paired_device_list("alice.01", Vec::new()),
            "No paired devices for session alice.01"
        );
    }

    #[test]
    fn a_session_name_promoted_away_still_selects_its_own_session() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let provisional = catalog.ensure_profile("worker-0")?;
        catalog.promote_to_user(&provisional, "alice.01")?;
        let args = SigningHostArgs {
            session: Some("worker-0".to_string()),
            ..SigningHostArgs::default()
        };

        assert_eq!(initial_session_name(&args, &catalog)?, "alice.01");
        Ok(())
    }

    #[test]
    fn a_base_path_with_several_provisioned_sessions_and_no_pointer_refuses_to_guess() -> Result<()>
    {
        let temporary = tempfile::tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        for name in ["alice.01", "bob.02"] {
            let profile = catalog.ensure_profile(name)?;
            std::fs::write(profile.path.join("accounts.json"), "{}")?;
        }

        let error = initial_session_name(&SigningHostArgs::default(), &catalog)
            .expect_err("an ambiguous base path must not select an identity");

        let message = error.to_string();
        assert!(message.contains("alice.01"), "{message}");
        assert!(message.contains("bob.02"), "{message}");
        assert!(message.contains("--session"), "{message}");
        Ok(())
    }

    #[test]
    fn a_lost_pointer_reselects_the_provisioned_session_instead_of_the_bootstrap_profile()
    -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let catalog = SessionCatalog::new(temporary.path().to_path_buf(), "testnet")?;
        let profile = catalog.ensure_profile("alice.01")?;
        std::fs::write(profile.path.join("accounts.json"), "{}")?;

        assert_eq!(
            initial_session_name(&SigningHostArgs::default(), &catalog)?,
            "alice.01"
        );
        Ok(())
    }

    #[test]
    fn an_auto_managed_signer_never_rotates_while_a_paired_host_depends_on_it() {
        assert!(signer_identity_may_rotate(true, 0));
        assert!(!signer_identity_may_rotate(true, 1));
        assert!(!signer_identity_may_rotate(true, 2));
        assert!(!signer_identity_may_rotate(false, 0));
    }

    #[test]
    fn trace_log_level_is_available_before_or_after_the_subcommand() {
        let before = Cli::try_parse_from(["truapi-host", "--log-level", "trace", "signing-host"])
            .expect("global log level before subcommand should parse");
        let after = Cli::try_parse_from(["truapi-host", "signing-host", "--log-level", "trace"])
            .expect("global log level after subcommand should parse");

        assert_eq!(before.log_level, Some(LogLevel::Trace));
        assert_eq!(after.log_level, Some(LogLevel::Trace));
        assert_eq!(LogLevel::Trace.as_filter(), "trace");
        assert_eq!(
            LogLevel::Trace.scoped_filter(),
            "warn,truapi=trace,truapi_host=trace"
        );
    }

    #[test]
    fn noisy_transport_targets_are_always_excluded_from_cli_logs() {
        assert!(!log_target_is_visible(terminal_ui::SSO_TRANSCRIPT_TARGET));
        assert!(!log_target_is_visible("rustls"));
        assert!(!log_target_is_visible("rustls::client::tls13"));
        assert!(!log_target_is_visible("tungstenite::protocol"));
        assert!(!log_target_is_visible(
            "tungstenite::protocol::frame::socket"
        ));
        assert!(log_target_is_visible("tungstenite::handshake"));
        assert!(log_target_is_visible("truapi::runtime"));
    }

    #[test]
    fn signing_host_exec_accepts_one_slash_command() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--auto-accept",
            "exec",
            "/help",
        ])
        .expect("exec slash command should parse");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };
        let Some(SigningHostAction::Exec { command }) = args.action else {
            panic!("expected exec action");
        };
        assert_eq!(command, "/help");
        assert_eq!(args.frame_listen, None);
    }

    /// The product id and the frame port are the two values the host and the
    /// product must agree on, and `dev` derives both so they cannot drift.
    #[test]
    fn dev_derives_the_product_id_from_the_app_port_and_wraps_a_command() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "dev",
            "--app-port",
            "5173",
            "--",
            "pnpm",
            "dev",
            "--host",
        ])
        .expect("dev should parse a wrapped command");
        let Command::Dev(args) = cli.command else {
            panic!("expected dev command");
        };

        assert_eq!(args.app_port, 5173);
        assert_eq!(args.port, DEFAULT_DEV_PORT);
        assert_eq!(args.product_id, None);
        assert_eq!(args.command, ["pnpm", "dev", "--host"]);
    }

    #[test]
    fn dev_runs_a_bare_host_when_no_command_is_wrapped() {
        let cli = Cli::try_parse_from(["truapi-host", "dev", "--product-id", "playground.paseo"])
            .expect("dev should parse without a wrapped command");
        let Command::Dev(args) = cli.command else {
            panic!("expected dev command");
        };

        assert_eq!(args.product_id.as_deref(), Some("playground.paseo"));
        assert_eq!(args.app_port, 3000);
        assert!(args.command.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn script_completion_removes_the_private_frame_socket_before_returning() -> Result<()> {
        struct UnusedRuntimeFactory;

        impl frame_server::ProductRuntimeFactory for UnusedRuntimeFactory {
            fn product_runtime(
                &self,
                _product: truapi::ProductContext,
                _sink: Arc<dyn truapi::FrameSink>,
            ) -> truapi::ProductRuntime {
                panic!("the completed script must not open a product connection")
            }
        }

        for script_failed in [false, true] {
            let frame_server = frame_server::bind(None).await?;
            let socket = PathBuf::from(
                frame_server
                    .endpoint()
                    .strip_prefix("ws+unix:")
                    .context("expected a private Unix socket")?,
            );
            let directory = socket.parent().context("socket has no directory")?;
            assert!(socket.exists());
            let product = frame_server::ProductSelection::new(
                "script-cleanup.testnet".to_string(),
                ProductExecutionKind::App,
            )?;
            let result = with_frame_server(
                Arc::new(UnusedRuntimeFactory),
                product,
                frame_server,
                async move {
                    anyhow::ensure!(!script_failed, "script failed");
                    Ok(())
                },
            )
            .await;

            assert_eq!(
                (result.is_err(), socket.exists(), directory.exists()),
                (script_failed, false, false),
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn dev_force_kills_a_term_ignoring_descendant_after_its_launcher_exits() -> Result<()> {
        const READY_PATH_ENV: &str = "TRUAPI_DEV_COMMAND_TEST_READY_PATH";

        if let Some(ready_path) = std::env::var_os(READY_PATH_ENV) {
            let _terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
            let _listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
            std::fs::write(
                ready_path,
                format!("{} {}", _listener.local_addr()?, std::process::id()),
            )?;
            return std::future::pending().await;
        }

        let temporary = tempfile::tempdir()?;
        let ready_path = temporary.path().join("descendant-ready");
        let executable = std::env::current_exe()?.to_string_lossy().into_owned();
        let launcher = r#"
"$1" --exact cli_tests::dev_force_kills_a_term_ignoring_descendant_after_its_launcher_exits --nocapture &
attempts=0
while [ ! -s "$TRUAPI_DEV_COMMAND_TEST_READY_PATH" ] && [ "$attempts" -lt 500 ]; do
    attempts=$((attempts + 1))
    sleep 0.01
done
test -s "$TRUAPI_DEV_COMMAND_TEST_READY_PATH"
"#;
        let started = std::time::Instant::now();
        let exit_code = run_dev_command(vec![
            "env".to_string(),
            format!("{READY_PATH_ENV}={}", ready_path.display()),
            "sh".to_string(),
            "-c".to_string(),
            launcher.to_string(),
            "sh".to_string(),
            executable,
        ])
        .await?;
        let ready = std::fs::read_to_string(&ready_path)?;
        let (address, pid) = ready
            .split_once(' ')
            .context("descendant readiness record is incomplete")?;
        let address = address.parse::<SocketAddr>()?;
        let port_was_released = std::net::TcpListener::bind(address).is_ok();
        if !port_was_released {
            let pid = pid
                .trim()
                .parse::<i32>()
                .ok()
                .and_then(rustix::process::Pid::from_raw)
                .context("descendant pid is invalid")?;
            let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
        }

        assert_eq!(
            (
                exit_code,
                port_was_released,
                started.elapsed() >= DEV_COMMAND_GRACE,
            ),
            (0, true, true)
        );
        Ok(())
    }

    #[test]
    fn frame_listen_explicitly_selects_tcp_websockets() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "pairing-host",
            "--frame-listen",
            "127.0.0.1:0",
        ])
        .expect("explicit TCP frame listener should parse");
        let Command::PairingHost(args) = cli.command else {
            panic!("expected pairing-host command");
        };

        assert_eq!(
            args.frame_listen,
            Some("127.0.0.1:0".parse().expect("valid socket address"))
        );
    }

    #[test]
    fn signing_host_rejects_script_and_exec_together() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--script",
            "smoke.ts",
            "exec",
            "/help",
        ])
        .expect("clap should parse parent options before exec");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };
        assert!(
            validate_signing_args(&args)
                .unwrap_err()
                .to_string()
                .contains("--script cannot be combined")
        );
    }

    #[test]
    fn signing_host_serve_needs_no_tty_and_refuses_one_shot_modes() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--serve",
            "--frame-listen",
            "127.0.0.1:9955",
            "--auto-accept",
        ])
        .expect("serve should parse");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };
        assert!(args.serve);
        assert!(validate_signing_args(&args).is_ok());

        let with_script = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--serve",
            "--script",
            "smoke.ts",
        ])
        .expect("serve with a script should parse");
        let Command::SigningHost(args) = with_script.command else {
            panic!("expected signing-host command");
        };
        assert!(
            validate_signing_args(&args)
                .unwrap_err()
                .to_string()
                .contains("--serve cannot be combined with --script")
        );

        let with_exec =
            Cli::try_parse_from(["truapi-host", "signing-host", "--serve", "exec", "/help"])
                .expect("serve with exec should parse");
        let Command::SigningHost(args) = with_exec.command else {
            panic!("expected signing-host command");
        };
        assert!(
            validate_signing_args(&args)
                .unwrap_err()
                .to_string()
                .contains("--serve cannot be combined with the exec subcommand")
        );
    }

    #[test]
    fn serve_ready_names_the_endpoint_and_the_approval_policy() {
        let prompting = terminal_ui::SystemEvent::ServeReady {
            url: "ws://127.0.0.1:9955".to_string(),
            auto_accept: false,
        }
        .human();
        assert!(prompting.contains("ws://127.0.0.1:9955"));
        assert!(prompting.contains("--auto-accept"));

        let accepting = terminal_ui::SystemEvent::ServeReady {
            url: "ws://127.0.0.1:9955".to_string(),
            auto_accept: true,
        }
        .human();
        assert!(accepting.contains("approved automatically"));
    }

    #[test]
    fn signing_host_accepts_a_startup_session() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--session",
            "alice",
            "exec",
            "/session",
        ])
        .expect("startup session should parse");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };

        assert_eq!(args.session.as_deref(), Some("alice"));
        assert!(validate_signing_args(&args).is_ok());
    }

    #[test]
    fn rerun_without_a_selected_script_does_not_create_or_edit_a_project() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let projects = temporary.path().join("scripts");

        let error = select_script(&ScriptCommand::Run(None), &projects, &mut None).unwrap_err();

        assert_eq!(
            (error.to_string(), projects.exists()),
            (
                "no script selected; use /script to create one or /script <path> to select one"
                    .to_string(),
                false
            )
        );
        Ok(())
    }

    #[test]
    fn clearing_sessions_keeps_managed_script_projects() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = Some(temporary.path().to_path_buf());
        let catalog = SessionCatalog::new(state_base_path(base.clone()), "testnet")?;
        let profile = catalog.ensure_profile(DEFAULT_SESSION_NAME)?;
        let projects = script_project_directory(base);
        let script = select_script_to_edit(&projects, &mut None)?;
        sessions::store_session_last_script(&profile.path, &script)?;

        catalog.clear(&SessionClearTarget::All)?;

        assert_eq!(
            (projects, script.is_file(), profile.path.exists()),
            (temporary.path().join("scripts"), true, false)
        );
        Ok(())
    }

    #[test]
    fn bare_script_selection_reuses_the_last_existing_script() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut last_script = None;

        let first = select_script_to_edit(temporary.path(), &mut last_script)?;
        let second = select_script_to_edit(temporary.path(), &mut last_script)?;
        assert_eq!(second, first);

        std::fs::remove_file(&first)?;
        let replacement = select_script_to_edit(temporary.path(), &mut last_script)?;
        assert_ne!(replacement, first);
        assert!(replacement.is_file());
        Ok(())
    }

    #[test]
    fn explicit_script_becomes_the_next_bare_script_selection() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let scripts = temporary.path().join("scripts");
        std::fs::create_dir_all(&scripts)?;
        let scratch = scripts.join("scratch.ts");
        std::fs::write(&scratch, "console.log('scratch');")?;
        let mut last_script = Some(scratch);
        let explicit = temporary.path().join("product-script.ts");
        std::fs::write(&explicit, "console.log('product');")?;

        let remembered =
            remember_script(Some(temporary.path()), &mut last_script, explicit.clone())?;
        let selected = select_script_to_edit(&scripts, &mut last_script)?;

        assert_eq!(remembered, explicit);
        assert_eq!(selected, explicit);
        assert_eq!(
            sessions::session_last_script(temporary.path())?.as_deref(),
            Some(explicit.as_path())
        );
        Ok(())
    }

    #[test]
    fn signing_host_rejects_managed_session_with_mnemonic() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--mnemonic",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "--session",
            "alice",
            "exec",
            "/session",
        ])
        .expect("clap should parse conflicting signer options");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };

        assert!(
            validate_signing_args(&args)
                .unwrap_err()
                .to_string()
                .contains("--session cannot be used")
        );
    }

    #[test]
    fn paired_device_management_rejects_an_ephemeral_mnemonic_session() {
        let cli = Cli::try_parse_from([
            "truapi-host",
            "signing-host",
            "--mnemonic",
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "exec",
            "/devices",
        ])
        .expect("clap should parse the command before lifecycle validation");
        let Command::SigningHost(args) = cli.command else {
            panic!("expected signing-host command");
        };

        assert_eq!(
            validate_signing_args(&args).unwrap_err().to_string(),
            "paired-device management is unavailable when launched with --mnemonic"
        );
    }

    #[test]
    fn saved_log_level_is_the_default_but_explicit_selection_wins() {
        assert_eq!(
            [
                resolve_log_level(None, None),
                resolve_log_level(None, Some(LogLevel::Debug)),
                resolve_log_level(Some(LogLevel::Trace), Some(LogLevel::Debug)),
            ],
            [LogLevel::Info, LogLevel::Debug, LogLevel::Trace]
        );
    }

    #[test]
    fn log_levels_display_in_lowercase() {
        assert_eq!(
            [
                LogLevel::Error,
                LogLevel::Warn,
                LogLevel::Info,
                LogLevel::Debug,
                LogLevel::Trace,
            ]
            .map(|level| level.to_string()),
            ["error", "warn", "info", "debug", "trace"]
        );
    }

    #[test]
    fn valid_rust_log_value_replaces_the_resolved_level() {
        let (filter, displayed) = startup_filter(Some(" warn,truapi=trace "), LogLevel::Info);
        let (_, fallback) = startup_filter(Some("truapi=verbose"), LogLevel::Debug);

        assert_eq!(
            (filter.to_string(), displayed, fallback),
            (
                "truapi=trace,warn".to_string(),
                "warn,truapi=trace".to_string(),
                "debug".to_string()
            )
        );
    }

    #[test]
    fn log_preference_uses_the_selected_host_base_path() {
        for arguments in [
            vec!["truapi-host", "pairing-host", "--base-path", "custom-state"],
            vec!["truapi-host", "signing-host", "--base-path", "custom-state"],
            vec!["truapi-host", "dev", "--base-path", "custom-state"],
        ] {
            let cli = Cli::try_parse_from(arguments).expect("host command should parse");

            assert_eq!(
                command_base_path(&cli.command),
                PathBuf::from("custom-state/v2")
            );
        }
    }

    #[test]
    fn log_level_preference_round_trips() -> Result<()> {
        let temporary = tempfile::tempdir()?;

        assert_eq!(load_log_level(temporary.path())?, None);
        store_log_level(temporary.path(), LogLevel::Debug)?;
        store_log_level(temporary.path(), LogLevel::Trace)?;

        assert_eq!(load_log_level(temporary.path())?, Some(LogLevel::Trace));
        Ok(())
    }

    #[test]
    fn invalid_saved_log_level_is_reported() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        std::fs::write(temporary.path().join(LOG_LEVEL_FILE), "verbose\n")?;

        let error = format!("{:#}", load_log_level(temporary.path()).unwrap_err());

        assert!(error.contains("invalid log level `verbose`"), "{error}");
        Ok(())
    }

    #[test]
    fn log_controller_persists_successful_updates() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let reloaded = Arc::new(std::sync::Mutex::new(None));
        let observed = reloaded.clone();
        let controller = LogController {
            reload: Arc::new(move |level| {
                *observed.lock().expect("reload observation lock") = Some(level);
                Ok(())
            }),
            base_path: temporary.path().to_path_buf(),
        };

        controller.set(LogLevel::Debug)?;

        assert_eq!(
            (
                *reloaded.lock().expect("reload observation lock"),
                load_log_level(temporary.path())?,
            ),
            (Some(LogLevel::Debug), Some(LogLevel::Debug))
        );
        Ok(())
    }

    #[test]
    fn log_controller_does_not_reload_when_persistence_fails() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base_path = temporary.path().join("not-a-directory");
        std::fs::write(&base_path, "occupied")?;
        let reloaded = Arc::new(std::sync::Mutex::new(None));
        let observed = reloaded.clone();
        let controller = LogController {
            reload: Arc::new(move |level| {
                *observed.lock().expect("reload observation lock") = Some(level);
                Ok(())
            }),
            base_path,
        };

        let error = controller.set(LogLevel::Debug).unwrap_err().to_string();

        assert!(error.contains("persist saved log level"), "{error}");
        assert_eq!(*reloaded.lock().expect("reload observation lock"), None);
        Ok(())
    }
}
