# TrUAPI Headless Host CLI v0.1 specification

- Status: as-built behavior reference
- Binary: `truapi-host`
- Implementation: `rust/crates/truapi-host-cli/`
- Protocol implementation: `truapi`

This document specifies the first complete version of the native headless
TrUAPI host CLI. It was derived from the Rust and TypeScript implementation,
the test suite, the checked-in compatibility reports, and observed runs of both
host roles.

The crate [README](README.md) is the user guide. This document is the complete
behavioral and engineering reference. It describes what v0.1 does, including
its operational limits; it does not contain a roadmap or requirements for
unimplemented features.

## 1. Purpose and scope

`truapi-host` runs real TrUAPI host roles without a browser UI, desktop shell,
phone automation service, or external signing bot. It is intended for:

- local product development;
- protocol and host diagnosis;
- direct signing-host tests;
- paired end-to-end tests; and
- generation of CLI host compatibility reports.

It embeds the real `truapi` dispatcher and host logic. Product scripts
use the public `@parity/truapi` client and exchange the same SCALE protocol
messages as a product connected to another host.

The CLI replaces the platform/operating-system seam with native implementations
for persistence, chain RPC, approvals, notifications, navigation, theme, and
terminal presentation. It also owns account onboarding, process orchestration,
the product-frame WebSocket bridge, and the Bun script runner.

It is local test infrastructure, not:

- a production wallet or secure custody product;
- a general-purpose mnemonic manager;
- a mock protocol server;
- a replacement for dot.li or Polkadot Desktop;
- a Chat, Coin Payment, or Payment backend; or
- an arbitrary-network RPC proxy.

## 2. Roles and runtime topologies

### 2.1 Pairing host

The pairing host is seedless and product-facing:

```text
product script or client
       │
       │ TrUAPI SCALE frames over WebSocket
       ▼
pairing-host
       │
       │ People-chain Statement Store / SSO
       ▼
signing-host
       │
       └── wallet entropy, product accounts, signing, aliases, proofs
```

The pairing host:

- serves product connections;
- starts SSO login and emits a `polkadotapp://pair?...` deeplink;
- persists the paired session and product state;
- receives a root entropy source from the signing host;
- delegates signing and other authority operations over SSO; and
- can run scripts before, during, or after pairing.

It never stores the signing host's raw mnemonic or raw root entropy.

### 2.2 Direct signing host

The signing host can also be product-facing:

```text
product script or client
       │
       │ TrUAPI SCALE frames over WebSocket
       ▼
signing-host
       │
       ├── local wallet entropy and product authority
       ├── People-chain Statement Store
       └── Bulletin and optional product-chain RPC
```

This path is used for focused diagnosis without SSO. It uses the same Rust
signing, account, entropy, Statement Store, Bulletin, and product runtime logic
as the paired path.

### 2.3 Ownership boundaries

`truapi` owns:

- protocol dispatch and SCALE encoding;
- product and role semantics;
- product-account derivation;
- signing and transaction construction;
- product-scoped entropy derivation;
- Ring-VRF aliases and proofs;
- SSO request/response encoding and transport;
- Statement Store proof, submission, and allowance logic;
- Bulletin preimage submission and resource allocation;
- subscriptions and runtime errors; and
- typed unavailable behavior for unsupported services.

`truapi-host-cli` owns:

- argument and slash-command parsing;
- the supported network presets (`paseo-next-v2`, `previewnet`);
- local signer selection and onboarding;
- local persistence and account-store locking;
- approvals and `--auto-accept`;
- the terminal UI and plain output;
- local pairing QR acquisition;
- product-frame WebSocket listening;
- session and product switching;
- child editor and Bun processes; and
- CLI-specific diagnostics.

Product scripts own:

- the TrUAPI calls under test;
- assertions over typed results;
- test output; and
- success or failure through normal completion or a thrown error.

## 3. Build, installation, and runtime dependencies

### 3.1 Build

From the repository root:

```sh
make headless
```

This builds:

- the Rust `truapi-host` binary; and
- the generated `@parity/truapi` TypeScript client used by the script runner.

The direct Cargo equivalent for the binary is:

```sh
cargo build -p truapi-host-cli
```

### 3.2 Installation

The published route is the installer script, which needs no Rust toolchain:

```sh
curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash
```

It resolves the current stable version from the `truapi-host-cli-stable`
release pointer, downloads the archive for the detected target
(`aarch64-apple-darwin`, `x86_64-unknown-linux-musl` or
`aarch64-unknown-linux-musl`), verifies its SHA-256, and lays out:

```
$XDG_DATA_HOME/truapi-host/versions/<version>/truapi-host
$XDG_DATA_HOME/truapi-host/versions/<version>/runner.js
$XDG_DATA_HOME/truapi-host/versions/<version>/script-types.d.ts
$XDG_DATA_HOME/truapi-host/current -> versions/<version>
~/.local/bin/truapi-host -> $XDG_DATA_HOME/truapi-host/current/truapi-host
```

`TRUAPI_HOST_VERSION`, `TRUAPI_HOST_INSTALL_DIR` and `TRUAPI_HOST_BIN_DIR`
override the version, the version store and the `PATH` directory.

The source route:

```sh
make headless install
```

The `install` target depends on `headless` and runs:

```sh
cargo install \
  --path rust/crates/truapi-host-cli \
  --bin truapi-host \
  --locked \
  --force
```

### 3.3 Runtime dependencies

Host-only commands need the installed Rust binary. Product scripts additionally
need `bun` on `PATH`, plus a runner (see below). A source build also needs the
repository's generated `@parity/truapi` TypeScript sources. Dev uses an existing
browser and the container bundle generated by `make headless` or `make cli-runner`.

The runner is resolved in this order: `TRUAPI_HOST_RUNNER`, then `runner.js`
next to the running binary, then `js/runner.ts` in the source checkout
(compiled from `CARGO_MANIFEST_DIR`). A managed version with a missing runner
fails instead of falling back to source code. After an update moves `current`,
the running binary continues using the runner from its own version directory.

A release archive ships `runner.js` and `script-types.d.ts` beside the binary.
The runner has `@parity/truapi` and the shared web API permission checks bundled
in, and the declaration file contains the matching generated client and
injected-global types, so an installed copy runs product scripts with no source
tree. New projects install their own SDK and editor dependencies. A source build
has no runner bundle and falls back to the checkout copies, whose relative
`@parity/truapi` import means the runner only works from a built tree.

The archive also ships `sandbox-assets/container.js` for dev. Source dev reads
that asset from `target/dist/sandbox-assets`; installed dev reads it beside the
runner. Scripts require neither browser assets nor a browser installation.

A `TRUAPI_HOST_RUNNER` override must provide a compatible
`script-types.d.ts` beside the selected runner when bare `/script` needs to
create an editor project.

Bun executes both the runner and the user script.

The binary has `--help` and `--version`.

### 3.4 Self-update

An install laid out by §3.2 keeps itself current. Every command except
`update` spawns a background check that:

1. does nothing unless the running executable resolves inside
   `<root>/versions/`, so a `cargo install` copy or a source build is never
   modified;
2. does nothing when `TRUAPI_HOST_NO_UPDATE` is set;
3. takes a non-blocking `<root>/update.lock`, and gives up if another process
   holds it;
4. does nothing if `<root>/update-check.json` records a check within the last
   four hours, and records the attempt *before* the network request so an
   unreachable release host is not retried on every invocation;
5. reads the published version from the `truapi-host-cli-stable` pointer and
   stops when `current` already selects it;
6. downloads the archive and its `.sha256`, refuses a digest mismatch, unpacks
   into `versions/<version>`, and renames a new symlink over `current`.

The running process is never replaced. A new version takes effect on the next
run, and the CLI logs one line when one is waiting. Versions other than the
running one and the active one are pruned.

The check runs alongside the command rather than delaying it, and the process
waits for it before exiting, so even a one-shot command completes the download
it started. A download in progress is announced, because an otherwise quick
command would seem to hang. That wait is bounded at 150 seconds, so a stalled
network cannot hold the CLI open; a download cut short leaves only a
`versions/.<version>.incoming` directory that the next attempt removes.

`truapi-host update` performs the same work synchronously, ignores the
four-hour throttle, and reports the outcome on stdout.

A binary outside that layout logs one line at startup naming itself a local
build and giving the installer command, because it never updates and is
otherwise indistinguishable from a managed install that is up to date.

The two routes shadow each other depending on `PATH` order, so each clears the
other: the installer removes a `cargo install` copy (via `cargo uninstall
truapi-host-cli`, falling back to deleting `$CARGO_HOME/bin/truapi-host`), and
`make headless install` runs the installer's `--uninstall` first. `--uninstall`
removes the version store and the `PATH` symlink, and only removes that symlink
when it points inside the version store.

## 4. Top-level command line

```text
truapi-host [--log-level <level>] [--debugger <url>] <command>
```

Commands:

| Command | Purpose |
| --- | --- |
| `pairing-host` | Run the seedless product-facing host. |
| `dev` | Run a development command against a loopback signing host and browser bridge. |
| `signing-host` | Run the wallet-local signing host. |
| `identity-check` | Probe dotNS identity records on Asset Hub for a mnemonic. |
| `register-name` | Register a full-person username via `DotnsGateway.register_name`. |
| `alloc-check` | Inspect or submit Statement Store allowance registration. |
| `pgas-check` | Inspect or submit an Asset Hub PGAS allowance claim. |

### 4.1 Global logging option

`--log-level` accepts:

- `error`
- `warn`
- `info`
- `debug`
- `trace`

The option is global and is accepted before or after a subcommand.
`TRUAPI_HOST_LOG` supplies the same per-process override. Without either, the
CLI restores the level saved by `/log` under `<base-path>/v2`, then falls
back to `info`. Command-line and environment overrides do not rewrite the saved
level.

If `RUST_LOG` contains a valid tracing filter, it takes precedence at startup
and the status bar shows its trimmed value. The interactive `/log` command
atomically saves the selected CLI level, replaces the active filter, and
updates the status bar to that level.

### 4.2 Global wire-debugger option

`--debugger <url>` streams every product frame to a wire debugger at a loopback
`ws://` address. `TRUAPI_DEBUGGER_URL` supplies the same value; clap resolves an
explicit flag over it.

The option is global, but only `pairing-host`, `dev` and `signing-host` resolve
it: the remaining commands emit no frames and open no sink. Resolution happens
before the frame listener binds, so a URL that is not `ws://` on `127.0.0.1`,
`localhost` or `[::1]` fails startup. A reachable debugger is not required, since
the sink dials lazily and reconnects.

Each of those three commands reports the outcome once, as a lifecycle event
rather than a log line: `Streaming wire frames to a debugger` with the endpoint
and the switch clap read it from, or `Wire debugger off`.

Each accepted connection gets its own channel id, `<product-id>#<n>`, so
concurrent peers under one host do not share a trace key.

## 5. `pairing-host`

```text
truapi-host pairing-host [options]
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--script <path>` | none | Run one JS/TS product script and exit with its status. |
| `--product-id <id>` | `headless-playground.dot` | Initial product scope. |
| `--frame-listen <socket>` | none | Opt into a TCP product WebSocket listener. When omitted, use a private per-process Unix socket. Port `0` selects an available TCP port. |
| `--base-path <path>` | section 12.1 | Base directory; managed state lives under its `v2/` subdirectory. |
| `--network <preset>` | `paseo-next-v2` | Select the complete endpoint/genesis preset (`paseo-next-v2`, `previewnet`). |
| `--auto-accept` | off | Approve platform confirmations automatically. |

Without `--script`, both stdin and stdout must be terminals. The command enters
the full-screen terminal UI and remains active until `/quit`, idle Ctrl-C, or
terminal input ends.

With `--script`, the command:

1. builds the pairing runtime;
2. binds and reports the product-frame listener;
3. starts the frame accept loop;
4. starts Bun with inherited stdio;
5. keeps the host alive until Bun exits;
6. stops the accept loop; and
7. exits with the child status.

The script must call `truapi.account.requestLogin()` or the operator must use
`/login` in interactive mode to initiate pairing.

There is no pairing-host `exec` subcommand.

## 6. `signing-host`

```text
truapi-host signing-host [options] [exec '<slash-command>']
```

| Option | Default | Behavior |
| --- | --- | --- |
| `--script <path>` | none | Run one direct product script and exit with its status. |
| `--product-id <id>` | `headless-playground.dot` | Initial product scope. |
| `--deeplink <url>` | none | Answer a pairing deeplink after initialization. |
| `--mnemonic <phrase>` | none | Use raw BIP-39 entropy as an ephemeral local signer. |
| `--account <name>` | none | Use one named account from the default account store. |
| `--session <name>` | remembered session | Select an exact username or the newest local session for a username base; restore or provision at interactive startup. In `exec`, select the command's session. |
| `--reserved-username <label>` | none | Full-person base name a newly created auto account reserves on dotNS alongside its lite username (§12.3). |
| `--base-path <path>` | section 12.1 | Base directory; managed state lives under its `v2/` subdirectory. |
| `--network <preset>` | `paseo-next-v2` | Select the complete endpoint/genesis preset (`paseo-next-v2`, `previewnet`). |
| `--frame-listen <socket>` | none | Opt into a TCP product WebSocket listener. When omitted, use a private per-process Unix socket. Port `0` is allowed. |
| `--auto-accept` | off | Approve platform confirmations automatically. |
| `--serve` | off | Run without a terminal UI, restore every paired host saved for the selected managed session, and stay up until stopped. |

`HOST_CLI_SIGNER_MNEMONIC` supplies `--mnemonic` when the option is omitted.

### 6.1 Argument conflicts

The CLI rejects these combinations with invocation status `2` before runtime
startup:

- `--script` with `exec`;
- `--serve` with `--script` or `exec`;
- `--mnemonic` with `--account`;
- `--mnemonic` with `--session`;
- `--mnemonic` with `--reserved-username`;
- `--account` with `--session`;
- `--account` with `--reserved-username`; and
- a `--reserved-username` that is not a full-person base label (lowercase ASCII
  letters only, 6 to 32 bytes).

The same conflicts apply when the mnemonic came from
`HOST_CLI_SIGNER_MNEMONIC`.

An explicit session name is validated before startup. Empty strings are treated
as absent after trimming.

### 6.2 Interactive mode

When `--script`, `--serve`, and `exec` are all absent, stdin and stdout must be
terminals. The signing host:

1. resolves the selected session and any locally cached signer;
2. creates the signing runtime;
3. activates a cached signer without a network onboarding round trip;
4. binds and reports the product-frame listener;
5. restores a responder for every paired host saved in the selected managed
   session when its signer is ready;
6. runs `--deeplink` as a `/pair` operation when supplied, otherwise restores or
   provisions an explicitly named `--session`; and
7. enters the command loop.

The startup operation runs through the interactive operation loop, with progress,
cancellation, and error reporting. Pairing prepares the selected signer itself;
cancelling startup does not queue another attempt.

Signer provisioning is otherwise lazy. Starting the UI without `--session`,
using `/help`, using `/product`, or inspecting sessions does not create a new account.

### 6.3 One-shot `--script`

The host binds its product listener, optionally starts a background responder
for the explicitly supplied `--deeplink`, ensures and activates a signer, runs
Bun with inherited stdio, stops that responder after the script, and exits with
the child status. It does not restore saved responders.

### 6.4 `signing-host exec`

```sh
truapi-host signing-host [parent options] exec '<slash-command>'
```

`exec`:

- parses exactly one slash command;
- starts the same signing runtime and product-frame listener;
- does not enter raw mode or the alternate screen;
- writes human output to normal stdout/stderr;
- optionally runs `--deeplink` in the background for the command lifetime;
- does not restore saved responders;
- stops the explicitly started responder when the command completes; and
- exits after the command.

Parent options must appear before `exec`.

`--session` selects the command's context without provisioning by itself.
Inspection, listing, help, and clearing stay local. `exec '/session <name>'`
explicitly restores or provisions that signer, including an unfinished current
session. An explicit `--deeplink` still performs its requested pairing operation.

For example:

```sh
truapi-host signing-host --session alice.01 exec '/devices'
truapi-host signing-host --session alice.01 exec '/devices --list'
truapi-host signing-host --session alice.01 exec '/devices --remove 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
truapi-host signing-host --session alice.01 exec '/devices --remove 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef --force'
```

`exec '/script'` needs a TTY because it opens an editor. In non-TTY execution,
use `exec '/script <path>'` instead. `/copy` and `/approval` are unavailable.
`/clear` and `/quit` are successful no-ops in one-shot mode.

`exec '/devices'` and `exec '/devices --list'` inspect the selected session's
saved pairings without starting their responders. `exec '/devices --remove
<statement-account-id>'` is an explicit removal and does not ask for another
confirmation. It submits `Disconnected` directly and removes local state only
after the statement store accepts it. Appending `--force` still attempts that
submission, but warns and continues with local cleanup if it fails.

### 6.5 `--serve`

`--serve` needs no TTY. It ensures and activates the signer, restores every
paired host saved for the selected managed session, optionally adds the host
from `--deeplink`, and serves product frames until Ctrl-C or external process
termination. `--auto-accept` is needed for confirmations because this mode has
no terminal prompt.

### 6.6 Top-level `dev`

```sh
truapi-host dev [options] [-- <development-command>...]
```

`dev` is the plain-browser development topology built on the signing host. It
needs no TTY, ensures and activates a signer, auto-accepts confirmations, binds
the loopback frame server and browser bridge, then starts the wrapped command.
When no command follows `--`, it serves the host until stopped.

| Option | Default | Behavior |
| --- | --- | --- |
| `--app-port <port>` | `3000` | Port of the development server. The default product id becomes `localhost:<port>`. |
| `--port <port>` | `9955` | Loopback port for product frames and `/bootstrap.js`. A product tag using another port must change with it. |
| `--product-id <id>` | `localhost:<app-port>` | Override the product scope. |
| `--network <preset>` | `paseo-next-v2` | Select the complete network preset. |
| `--session <name>` | remembered session | Restore or create a persistent signing-host session. |
| `--mnemonic <phrase>` | none | Use a disposable testnet signer instead of the managed session. `HOST_CLI_SIGNER_MNEMONIC` supplies the same value. |
| `--base-path <path>` | section 12.1 | Base directory; managed state lives under its `v2/` subdirectory. |

The product includes a development-only blocking tag before product code:

```html
<script src="http://127.0.0.1:9955/bootstrap.js"></script>
```

The script installs the shared browser container and publishes
`window.__HOST_API_CLIENT__`. SDK calls and authorization requests use this
stable client, sharing one WebSocket and one Rust execution. The container
consumes `window.__truapi_localhost` during startup. HTTP and WebSocket share the
same TCP port. After a disconnect, the SDK replaces the socket. Interrupted calls
fail, subscriptions end, and neither is replayed. Older SDKs use the compatibility
`window.__HOST_API_PORT__` MessagePort, which requires a page reload after a
disconnect.
Production builds must omit the tag.

On Unix the wrapped command is the leader of a process group retained by the
CLI. A natural direct-launcher exit preserves its status and still cleans up
descendants. On CLI SIGINT or SIGTERM, cleanup reports status 130. Both paths
send SIGTERM to the group, wait up to five seconds while reaping the direct
child, then send SIGKILL and wait again if any group member remains. On
non-Unix platforms the CLI stops and reaps the direct child.

The first blocking bridge tag installs the shared `js/container` bundle before
application code. Existing SDKs can start without a product dependency update;
recovery through a retained client requires the updated SDK. SDK calls and
authorization requests share one WebSocket and Rust execution. `/script` imports
the same web API permission wrappers
into Bun. Dev preserves the app server URL, native assets and hot reload; no
app proxy is involved.

## 7. Product identifiers and switching

Accepted product identifiers are:

- a name ending in a dotNS TLD (`.dot`, `.paseo` or `.testnet`);
- `localhost`; or
- a string beginning with `localhost:`.

Identifiers are trimmed, Unicode-NFC normalized, and lowercased. For example,
`" Dotli.DOT "` becomes `dotli.dot`.

Other identifiers, including an ordinary `example.com`, are rejected.

The product id scopes:

- product accounts and signatures;
- Ring-VRF context and cross-product policy;
- derived entropy;
- local product storage;
- permissions and authority checks;
- product-frame runtime context; and
- `host.productId` and `host.productAccount()` in scripts.

`/product` prints the current normalized id.

`/product <id>` changes only the process-local product selection. It does not
change:

- the network;
- the active signer or paired user;
- the signing session;
- the SSO relationship; or
- another product's stored data.

Changing the product invalidates all active product WebSockets. Reload browser
dev pages; new connections receive the new product context. Selecting the
already-current normalized id does not reset connections.

Product-scoped entropy is intentionally different for different product ids
even when the account and caller context bytes are identical.

## 8. Slash commands

Commands start with `/`. There are no `q`, `quit`, `exit`, or non-slash aliases.

| Command | Pairing host | Signing host | Behavior |
| --- | :---: | :---: | --- |
| `/script` | yes | yes | Edit and run the remembered script, creating a project when needed. |
| `/script <path>` | yes | yes | Remember and run an existing JS/TS script. |
| `/script --run` | yes | yes | Run the remembered script without editing. |
| `/script --edit` | yes | yes | Edit without running. |
| `/script --new [directory]` | yes | yes | Create a project in a new directory, then edit and run. |
| `/login` | yes | no | Start or join pairing for the current product, show its QR code, and copy the new link. |
| `/logout` | yes | no | Disconnect and clear the old pairing identity/history. |
| `/pair` | no | yes | Wait for a pairing QR image from Ctrl-V, terminal paste, or drag-and-drop. TUI only. |
| `/pair <image-path>` | no | yes | Decode a pairing QR from a PNG, JPEG, or WebP image. |
| `/pair <url>` | no | yes | Validate and answer a `polkadotapp://pair?...` link. |
| `/devices` | no | yes | List paired devices saved for the active managed session. |
| `/devices --list` | no | yes | List paired devices saved for the active managed session. |
| `/devices --remove <statement-account-id>` | no | yes | Disconnect and remove one paired device by its 32-byte statement account ID. |
| `/devices --remove <statement-account-id> --force` | no | yes | Attempt to disconnect one paired device, then remove its local pairing even if notification fails. |
| `/approval` | no | yes | Print the current manual or automatic approval mode. TUI only. |
| `/approval manual` | no | yes | Prompt for every future confirmation. TUI only. |
| `/approval automatic` | no | yes | Approve every future confirmation automatically. TUI only. |
| `/product` | yes | yes | Print the current product id. |
| `/product <id>` | yes | yes | Switch product and reset product connections. |
| `/session` | no | yes | Show current session, user, and path. |
| `/session <name>` | no | yes | Switch to or create and provision a session. |
| `/session --mnemonic "<phrase>"` | no | yes | Import an existing ring member into a durable local session. |
| `/session --list` | no | yes | List network-scoped user sessions and mark the active one. |
| `/session --clear <name>` | no | yes | Permanently clear one network-scoped signing session. |
| `/session --clear-all` | no | yes | Permanently clear all signing sessions for the current network. |
| `/log <level>` | yes | yes | Save and replace the runtime log filter. |
| `/help` | yes | yes | Show role-specific commands and key bindings. |
| `/clear` | yes | yes | Clear the retained visible transcript. |
| `/copy` | yes | yes | Copy the retained, redacted transcript. TUI only. |
| `/quit` | yes | yes | Leave the command loop. |

The shared parser recognizes every command, then the active role rejects
commands it cannot execute. `/pair <url>` performs a fast prefix check; the
Rust core then fully decodes and validates the V2 handshake. Any other single
quoted or escaped `/pair` argument is treated as an image path.

`/devices` and `/devices --list` are equivalent. They sort peers by statement
account ID and print each ID with any available host and platform metadata.
`/devices --remove` accepts exactly one 32-byte hexadecimal statement account ID
with an optional `0x` prefix and an optional trailing `--force`. Interactive
removal uses the `[y/N]` approval and describes that only the selected peer is
affected. `exec` removal runs directly. Both modes submit one `Disconnected`
message before local cleanup, allowing up to 30 seconds for the statement store
to accept it. This does not wait for a peer acknowledgement. If submission fails
or times out, ordinary removal preserves the saved pairing, responder, and
allowance-renewal target. Forced removal emits
an unfiltered warning and continues with local cleanup, so the remote host may
continue to show stale connected state, but it cannot reach a responder on this
signing host.

Unknown commands, missing required arguments, invalid log levels, invalid
products, invalid session names, and arguments passed to no-argument commands
produce explicit errors.

### 8.1 Pairing QR image input

Bare `/pair` is available only in the interactive signing-host TUI. It starts a
terminal waiting state that accepts Control-V or the terminal's normal paste
shortcut. In both cases the TUI reads RGBA pixels directly from the
operating-system clipboard. A bracketed text paste triggers that clipboard read
rather than carrying the pixels itself, so image paste continues to work through
tmux without a terminal-specific image escape protocol.

A dropped file is accepted as a raw, quoted, shell-escaped, or `file://` path.
Bracketed path paste starts decoding immediately. A terminal that inserts the
path as individual key events leaves it in the command bar for Enter to submit.
Text that is neither backed by an image clipboard nor a readable file leaves
`/pair` waiting with recovery instructions.

`/pair <image-path>` reads a PNG, JPEG, or WebP file in either interactive or
one-shot mode. Clipboard and file pixels remain in memory and are not written to
a temporary file.

Before decoding, the implementation enforces all of these boundaries:

- nonzero dimensions of at most 8192 pixels per edge;
- at most 24 million pixels and an exact four RGBA bytes per pixel;
- at most 64 MiB for an encoded image file; and
- at most 256 MiB of image-decoder allocation.

Alpha is composited over white before conversion to grayscale. The normal QR
detector runs against both polarities. A bounded fallback recognizes horizontal
and vertical finder-pattern ratios, groups the three axis-aligned finder marks,
samples module centers, and passes the sampled matrix through the same QR error
correction and payload decoder. This fallback supports the circular finder marks
and light-on-dark presentation used by Polkadot app screenshots without a native
or platform-specific barcode library. Candidate lines, finder groups, dimensions,
and QR versions are bounded before combinatorial work.

The result distinguishes no QR code, an unrelated QR code, and multiple distinct
valid pairing codes. Exactly one decoded value must begin with
`polkadotapp://pair?handshake=` and pass the Rust core V2 handshake decoder. It is
then passed directly to the same pairing responder used by `/pair <url>` and is
never logged. QR decoding runs on the blocking-task pool rather than the
asynchronous I/O executor.

A clipboard access or conversion error leaves `/pair` waiting so the operator
can copy another image and paste again. Ctrl-C cancels the waiting state and
returns to the command bar. Non-interactive `exec '/pair'` fails with
instructions to provide `/pair <image-path>` or `/pair <url>` instead.

## 9. Terminal UI

### 9.1 Layout

Both roles use the same full-screen ratatui/crossterm surface:

```text
scrollable transcript

command completion list, when open
› command input or idle placeholder
TrUAPI <role> host · 👤 <state-or-name> · 🌐 <network> · 📦 <product> · log <value>
```

The role label is omitted at narrow widths so user, network, and product remain
visible. Values are ellipsized to fit, with the product consuming the remaining
space after the user, network, and fixed log value. The session name is not shown
separately from the resolved user. Idle command guidance appears as a dim
placeholder inside the empty prompt instead of consuming status-bar space.
Operational hints temporarily use the right side of the status line while a
command, approval, completion menu, or scroll is active.

The composer:

- has one column of horizontal padding when width permits;
- adds vertical padding on terminals at least seven rows high;
- blends a subtle surface color from `COLORFGBG`;
- uses true color when `COLORTERM` is `truecolor` or `24bit`;
- falls back to an ANSI-256 approximation; and
- becomes unstyled when `NO_COLOR` exists.

### 9.2 Transcript

The transcript contains:

- host lifecycle events;
- submitted commands;
- script stdout and stderr;
- approvals;
- SSO request/response summaries; and
- tracing output allowed by the active log filter.

Submitted commands become bold, full-width divider titles. `/pair` arguments
are rendered as `/pair <pairing link>`.

Status symbols are:

| Symbol | Meaning |
| --- | --- |
| `•` | informational |
| `◌` | running |
| `✓` | success |
| `!` | warning |
| `×` | failure |
| `–` | cancelled |

Running activities are keyed and updated in place. For example, `Script
running` becomes `Script finished` or `Script failed`, and pairing progresses
from link generation through authentication to its final state.

### 9.3 Input and completion

- Typing `/` opens role-specific completion.
- Up/Down cycles completion while it is visible.
- Up/Down navigates process-local command history when completion is closed.
- Tab accepts the selected completion.
- Enter first accepts a differing selected completion; a later Enter submits.
- `/script` followed by a space completes actions and filesystem entries.
- `/devices` followed by a space completes `--list` and `--remove` for the
  signing host.
- `/approval` followed by a space completes `manual` and `automatic` for the
  signing host.
- `/session` followed by a space completes known signing sessions, `--list`,
  `--mnemonic`, `--clear`, and `--clear-all`; `/session --clear ` completes
  known names.
- Left/Right, Home/End, Backspace, and Delete edit by Unicode character.
- Long input scrolls horizontally and retains a native terminal cursor.
- Bracketed paste is enabled; pasted control characters are discarded.
- At most eight completion rows are visible.

Command history is in memory only and disappears when the process exits.
Mnemonic import commands are excluded from history, debug rendering, busy
labels, and transcripts. Their phrase is masked character-for-character in the
command bar. `exec` cannot hide a phrase from the invoking shell's history or
the operating system's process arguments.

### 9.4 Scrolling and cancellation

- Ctrl-U scrolls up by half the transcript viewport.
- Ctrl-D scrolls down by half the viewport.
- End moves the input cursor to the end and resumes latest-output view.
- Esc dismisses completion.
- Ctrl-C clears non-empty input.
- Idle Ctrl-C exits the command loop when input is empty.
- Busy Ctrl-C drops the active operation future.

Only one operator command runs at once. Input may be prepared while a command
runs, but Enter reports that another command is active. Host events and
approval input continue to be processed during the operation.

Captured script children use `kill_on_drop`, so cancelling an interactive
script terminates Bun. Pairing-host `/login` additionally calls the core's
pairing cancellation method.

### 9.5 Approvals in the TUI

An approval temporarily saves and clears the command draft. The operator can:

- press `y` to approve an action;
- press `o` for Allow once or `a` for Allow always on a permission;
- press `n` or Esc to reject; or
- type the corresponding word and press Enter. Letter shortcuts accept either case.

Invalid typed answers show the available choices. The saved draft is restored
after the decision. Approval requests are serialized by the platform prompt
lock.

### 9.6 Clipboard and redaction

`/copy` lazily opens the system clipboard and copies plain transcript text
without the full-screen UI. Complete pairing links are replaced by
`<pairing link>`.

Operator `/login` copies the first generated pairing link automatically. A
clipboard failure is reported as a warning and does not cancel pairing.
Product-driven `requestLogin()` does not automatically copy its link.

Every pairing link received by the interactive UI is followed by a solid
half-block QR code that encodes the exact deeplink. Each terminal cell carries
one module column and two module rows. QR rows use an explicit white background,
black modules, and a four-module quiet zone. If the terminal cannot fit the QR
without wrapping or clipping, the UI keeps the raw link and reports the
required columns and rows. Encoding failure is reported without cancelling
pairing. Streaming output and copied transcripts remain text-only.

### 9.7 Output safety and bounds

Captured script and log text is sanitized before rendering:

- CSI escape sequences are removed;
- OSC escape sequences are removed;
- other control characters are removed except newline and tab; and
- individual child-output lines are truncated at 16 KiB.

Consequently Chalk color, bold, and other ANSI styling are not rendered inside
the TUI. One-shot `--script` inherits stdout and can render ANSI normally.

The retained transcript is pruned from the oldest item when any limit is
exceeded:

- 10,000 feed items;
- 10,000 logical lines; or
- 1 MiB of retained plain text.

Adjacent output is chunked at 256 lines or 64 KiB.

## 10. Product scripts

### 10.1 Execution contract

A product script is a JavaScript or TypeScript ES module executed by Bun.
Before importing it, the runner:

1. reads its required environment;
2. opens the product-frame WebSocket over its Unix or TCP endpoint, with a
   15-second connection timeout;
3. creates the public `@parity/truapi` client and exposes the standard host
   discovery interface for SDK imports;
4. injects the script globals;
5. installs the shared container's fetch and WebSocket permission wrappers,
   plus its XHR wrapper when that API exists; and
6. switches to the managed project root, or restores the caller's working
   directory for an unmanaged script, and imports the absolute script URL.

Top-level module code is awaited. If the module's default export is a function,
the runner calls and awaits it with the host context.

Public SDK calls and authorization requests share one transport and Rust
execution, preserving Allow once, Allow always and Deny semantics. These wrappers
are also used by the full browser container loaded by dev's first blocking
bootstrap tag.

The runner exposes `window.__HOST_WEBVIEW_MARK__`, `__HOST_API_CLIENT__`, and
the legacy `__HOST_API_PORT__` adapter over that same connection. Published Product
SDK imports discover the host automatically. Scripts using an older SDK's
legacy port must be rerun after a host disconnect.

Scripts retain Bun/Node filesystem, environment, subprocess and module imports.
Native networking APIs can bypass the web API wrappers; this is not
operating-system isolation. No browser runtime or product bundler is involved.
The transport and provider are disposed on success or failure.

### 10.2 Injected globals

```ts
declare const truapi: TrUApiClient;

declare const host: {
  productId: string;
  productAccount(index?: number): ProductAccountId;
};

declare function assert(
  condition: unknown,
  ...message: unknown[]
): asserts condition;
```

`host.productAccount()` defaults to derivation index `0` and uses the exact
active product id.

`assert` joins string arguments directly and formats other values with
`node:util.inspect` without color. A false condition throws either the joined
message or `assertion failed`.

### 10.3 Internal child environment

The Rust parent sets:

| Variable | Meaning |
| --- | --- |
| `TRUAPI_FRAME_URL` | Bound product-frame endpoint: `ws+unix:/path` by default or `ws://address` in TCP mode. |
| `TRUAPI_PRODUCT_ID` | Normalized active product id. |
| `TRUAPI_SCRIPT` | Canonical absolute script path. |
| `TRUAPI_CLI_HOST_ROLE` | `pairing-host` or `signing-host`. |
| `TRUAPI_SCRIPT_CWD` | Managed project root, or caller working directory for an unmanaged script, selected before importing it. |

These variables are runner internals, not CLI configuration inputs.

The launcher runs from its trusted directory with automatic Bun config,
dotenv loading, and macros disabled. `--install=fallback` uses installed
packages first, then downloads missing npm imports into Bun's cache.
Product-side configuration cannot run code before the web API wrappers are
installed. Editor types still require dependencies installed in the project.

### 10.4 Script status

- Successful completion exits `0`.
- A thrown error or rejected promise is printed as `[script error] ...` and
  exits `1`.
- Failure to open the product socket within 15 seconds exits `2`.
- Failure to locate the runner or its declaration bundle, canonicalize the
  script, or spawn Bun is a CLI error.

The CLI emits `Script running` before Bun starts and `Script finished` or
`Script failed` afterward.

One-shot `--script` preserves the child's normal numeric status. Interactive
script failures are displayed and the TUI remains active. `exec '/script
<path>'` reports the child code but returns the CLI's general error status when
the child failed.

### 10.5 Remembered scripts and editor behavior

`/script <path>` resolves a relative path against the CLI process's current
working directory, remembers the resulting absolute path in the current host
session, and runs it.

A later bare `/script`:

1. reuses the remembered file when it still exists;
2. otherwise creates a unique project under `<base-path>/scripts/`, where
   base-path is the configured/default base before the `v2` suffix;
3. stores that selection;
4. installs missing managed dependencies with visible progress, then leaves the TUI;
5. opens the file in the configured editor;
6. restores the TUI; and
7. runs the script when the editor exits successfully.

Editor selection order:

1. non-empty `VISUAL`;
2. non-empty `EDITOR`;
3. `notepad` on Windows; or
4. `vi` elsewhere.

The editor specification is parsed with shell-like quoting but is launched
directly, without a shell. Values such as `EDITOR='code --wait'` work.

An editor failure retains the script and does not run it.

Legacy session-local scripts still store their filename in `session.json`.
Managed projects and other external scripts store absolute paths. Projects
survive both session promotion and clearing. Mnemonic sessions also create
durable projects, but remember their selection only for the current process.
A missing managed entrypoint can be recovered through the project's
`truapiHost.script` metadata after an intentional rename. Otherwise bare
`/script` creates a new project; `--run` reports a missing selection instead.

A new project contains `script.ts`, `script.types.d.ts` copied from the
selected runner, `package.json`, and `tsconfig.json`. The manifest marks the
project with `"truapiHost": { "script": "script.ts" }`. This entry must name
a relative path inside the project. When the nearest package.json carries
this metadata, its directory is the execution cwd, whether invoked explicitly
or through remembered state.
Ordinary package directories without this metadata retain the inherited cwd
and are never installed or modified by the CLI.

The starter uses the Product SDK quickstart unchanged: `createApp` with
`name: "my-app"` and `logLevel: "info"`, `wallet.connect()`, and a local-storage
write and read of `lastVisit`. The host product id must match `my-app.dot`.
Default cloud storage uses Paseo, also the CLI's default network. A signed-out
wallet can return an empty account list.

The template requests `latest` for the Product SDK, TypeScript, and Bun editor
types, with resolved versions recorded in the project's lockfile. It has no
protocol override. Users can choose their SDK version with `bun add`.
SDK imports resolve from the local installation, which also provides editor
types and `bun run typecheck`. The editor opens from the managed project root.
Raw TrUAPI scripts can import the adjacent declarations locally, allowing
multiple scripts to compile together.

Dependency setup runs `bun install`, with `--frozen-lockfile` when a Bun
lockfile exists. Only a successful installation records its manifest and
lockfile fingerprint under `node_modules`; missing packages, a changed
manifest/lockfile, or an interrupted install triggers setup again. Reopening
a complete installation does not invoke the package manager. Failed or
cancelled setup preserves source files and remains retryable. Host updates
never rewrite existing manifests, lockfiles, scripts, or declaration files.

`--edit` stops after the editor closes. `--new [directory]` creates another
project and refuses to overwrite an existing destination. Recognized flags
are reserved; `/script -- <path>` selects a path that begins with one.
Path arguments retain spaces without shell tokenization.

The top-level `--script` option does not update remembered `/script` state.

### 10.6 Shipped scripts

`rust/crates/truapi-host-cli/js/scripts/` contains:

| Script | Purpose |
| --- | --- |
| `battery.ts` | Run every generated Playground example and write the role-specific compatibility report. |
| `whoami.ts` | Print the primary username. |
| `signing-smoke.ts` | Focused product-account signing test. |
| `ring-vrf-smoke.ts` | Verify RFC-0024 registration, listing, alias, non-membership proof, and direct signing behavior. |
| `preimage-smoke.ts` | Exercise Bulletin preimage submission and lookup. |
| `smart-contract-allowance-smoke.ts` | Requests a PGAS allowance for product account index 0 and reports the outcome. |
| `chat-battery.ts` | Run host-backed Chat screening diagnostics and write a report. |

`battery.ts` writes to `explorer/diagnosis-reports/spa/<role>-cli.md` unless
`TRUAPI_BATTERY_REPORT_PATH` overrides the destination. `scripts/battery.sh` in
the repository root produces both reports in one invocation: it runs the direct
signing-host phase, then starts a pairing host and answers its emitted link
with a second signing host using the same product id and forwarded host flags
so the paired phase can complete. Known unsupported service families remain
reported as failures but do not fail the process; any other generated-example
failure is a nonzero exit. Its custom reporter uses terminal color when stdout
is a TTY or `FORCE_COLOR` is nonzero, unless `NO_COLOR` exists.

Beyond the generated examples, `battery.ts` runs one hand-written
`Resource Allocation/auto_signing_e2e` case: it allocates `AutoSigning`, then
requires two `sign_vrf` calls for the granting product to succeed without any
confirmation being consulted. When `TRUAPI_APPROVALS_LOG` names a file, every
host process appends one `<approved|denied> <action>` line per decided
confirmation there before the confirmation resolves; the case reads the file
to prove the prompt-free window (and is reported as skipped when the variable
is unset). `scripts/battery.sh` exports the variable for each phase, sharing
one file across both paired-phase host processes.

## 11. Pairing lifecycle

### 11.1 Login

Login may start from:

- a product calling `truapi.account.requestLogin()`;
- operator `/login`; or
- an already-running product script.

The pairing host generates or reuses its current pairing device identity,
publishes the handshake proposal through the People-chain Statement Store, and
emits a `polkadotapp://pair?...` link.

The observable states are:

```text
disconnected
  -> pairing link ready
  -> authenticating
  -> paired with <user>
```

Failures become `Pairing failed`. A rejected login returns `Rejected`; a
connected session can return `AlreadyConnected`.

`/login` uses the product selected at the moment the command starts. Ctrl-C
cancels that login attempt.

### 11.2 Signing-host response

Before a signing host answers a link, it:

1. ensures a signer;
2. decodes the V2 handshake;
3. derives its RFC-0022 `uid.<tld>` identity account;
4. reads the pairing device Statement Store account from the proposal;
5. finds the signer's rings through the pairing-attestation bootstrap `peopl.<tld>`
   keys, index 0 for `People` and index 1 for `LitePeople`, scanning back from
   the current ring in each (RFC-0024 operational key selection uses the
   registry instead);
6. grants or reuses Statement Store allowance for the identity account;
7. grants or reuses allowance for every saved pairing device and the
   candidate;
8. submits the encrypted handshake response;
9. after a successful submission, stores the candidate for a managed session;
   and
10. starts the resumable SSO responder.

For a managed session, `/pair` stores the candidate in `paired-hosts.json`
after the handshake succeeds and before starting the resumable responder. The
statement account ID is the key. Pairing the same host again updates its public
key and display metadata and replaces only that host's responder. Responders for
other statement accounts keep running.

Interactive mode and `--serve` restore every saved peer for the selected
session. Background responders treat transient failures and ended subscriptions
as retryable, with exponential backoff capped at 30 seconds. A remote
`Disconnected` protocol message stops retrying and removes only that peer from
`paired-hosts.json` and allowance renewal. Other peers remain saved and active.

Each responder uses a durable, versioned replay ledger scoped by the root
public key, peer statement account ID, and peer encryption public key. A
request is recorded as started before its messages execute and as completed
afterward. Either state suppresses execution of an unexpired duplicate while
still publishing the statement-level success acknowledgement. A duplicate
`Disconnected` request still terminates and removes that peer. Expired entries
are pruned. Missing or overly distant peer expiry is retained for at most the
seven-day SSO statement lifetime. At 1,024 live entries the ledger rejects a new
request before executing it instead of evicting an unexpired replay marker.

In `exec`, `/pair` waits for the responder to finish. With `--deeplink` plus
another `exec` command or a script, the responder runs only for that command's
lifetime and is then stopped. One-shot `exec` and `--script` do not restore
other saved responders.

### 11.3 Logout and re-pairing

Pairing-host `/logout`:

- invalidates an in-flight login;
- disconnects the active account-authority session;
- clears the persisted auth session;
- tries to publish the disconnected SSO message;
- deletes `PairingDeviceIdentity`; and
- deletes `LastProcessedPairingStatement`.

The next login generates a fresh pairing keypair/topic and can pair with a
different signing host.

Logout does not clear:

- product storage;
- non-auth core state;
- scripts;
- the selected product; or
- another user's identity-scoped directory.

## 12. Signing identities, accounts, and sessions

### 12.1 Base path

Host commands choose their base path in this order:

1. explicit `--base-path`;
2. `TRUAPI_HOST_BASE_PATH`;
3. `$XDG_STATE_HOME/truapi-host`;
4. `$HOME/.local/state/truapi-host`; or
5. `.truapi-host`.

The CLI appends `v2` to the selected base path before accessing any managed
state, including accounts, sessions, pairings, core and product storage,
managed scripts, and log preferences. This applies to custom base paths too.
The signing session catalog converts the resulting relative path to an
absolute path at startup. Pairing storage uses the resulting path directly.

Previous state outside `v2` is left untouched and unused. The CLI starts normal
onboarding with fresh identities and pairings because the previous `.dot`
identities cannot be reused with network-specific reserved keys. There is no
state migration; see the [state directory guide](README.md#state-directory).

### 12.2 Signer selection

Signing-host signer selection order is:

1. explicit `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC`;
2. explicit `--account`;
3. the first attested, non-exhausted auto account for the network and current
   Statement Store period;
4. the first pending auto account for the network; or
5. a newly generated auto account.

Explicit mnemonic mode:

- parses a 12/15/18/21/24-word BIP-39 phrase;
- uses the raw BIP-39 entropy, not the PBKDF2 seed;
- does not read or write an account record;
- has no cached username;
- reports the session as `ephemeral`; and
- disables commands that switch or import managed sessions.

It can answer a deeplink for the current process, but it has no managed profile
in which to save the peer. It does not restore paired hosts, and `/devices` is
unavailable.

Explicit account mode looks up a named record in the default account store and
ensures its on-chain identity and ring readiness. It is not considered
auto-managed for slot rotation.

### 12.3 Auto-account onboarding

A new auto account:

1. acquires `accounts.json.lock` and validates its username base before onboarding;
2. generates a 12-word mnemonic;
3. derives the RFC-0022 `uid.<tld>` index-0 sr25519 identity account;
4. chooses `auto-<n>` as its local name;
5. checks that the requested Lite username base has an available numerical
   alias;
6. saves a pending account record;
7. builds and submits identity-backend registration proofs, including the dotNS
   gateway reservation signature timestamped with Asset Hub chain time. A
   reserved base name (`--reserved-username`) must be a full-person label and
   unminted on the dotNS registrar (`DotnsRegistrar.ownerOf` reverts for its
   node under the network TLD): a reservation for a registered name could never
   be claimed and would hold that stem's reservation queue for the whole
   reservation window;
8. polls the dotNS contracts on Asset Hub for the final `name.discriminator`;
9. waits for inclusion in a personhood ring; and
10. marks and saves the account as attested.

Identity and ring polling each allow 30 attempts with four seconds between
attempts. Identity-backend HTTP clients use a 30-second timeout.

The backend's username routes are bearer-gated. Unless
`HOST_CLI_IDENTITY_BACKEND_TOKEN` supplies one, the CLI mints an access token
for the mnemonic's RFC-0022 `uid.<tld>` account. It takes a challenge from
`auth/challenges`. It answers `auth/token` with an sr25519 proof over
`SHA256(challenge || clientId || SHA256(body))`, signed by that identity key.
The backend requires the JWT subject to equal `candidateAccountId` on
`POST /usernames`; using the same key for availability and registration also
prevents a pre-registration request from caching a token for another subject.
Tokens are cached per backend and identity account. A cached token rejected with
401 is evicted and minted again for the same account before the request is
retried once. The CLI accepts both the legacy flat availability map and the
dotSpark v1 envelope whose per-name value carries a `status` field.

The default Lite username prefix is `headless`. For a non-default session, the
prefix is its lowercase ASCII letters with digits and separators removed. A new
account requires at least six letters in that base. `/session foo` fails
immediately with a too-short username-base error, before network onboarding.
`--session` selects the base; there is no separate username-prefix option.
This validation applies to account creation; existing saved accounts and
aliases still restore by their original names.

The backend assigns the numerical alias. An available base is used unchanged;
the CLI does not append random suffixes or try alternative bases. It saves the
base in the pending account before registration, so retries reuse it. Exhausted
bases report an error. Invalid or unexpected availability responses are errors.

### 12.4 Cached startup

An attested account with a resolved Lite username can be loaded and activated
from `accounts.json` without contacting the identity backend or checking ring
membership on every restart. The current Statement Store period is still used
to skip locally marked exhausted accounts.

Imported accounts are stored with an explicit `imported` origin and are never
eligible for the auto-account pool or slot rotation. `session.json` binds a
durable session to its exact local account name even when it has no dotNS
username, so restart selects the imported record instead of whichever auto
account happens to appear first.

### 12.5 Statement Store slot rotation

Before pairing, allowance is registered for both the signing wallet and device
accounts. If registration reports no free slot, the signer is auto-managed, and
the session has no saved pairing, the CLI:

1. records the current Statement Store period in that account;
2. selects or creates another account;
3. activates the new signer; and
4. retries pairing preparation.

At most eight rotations are attempted. Explicit mnemonic and explicit account
modes return the slot error instead of changing identity. A managed session
with one or more saved pairings also preserves its signer. Pairing another
device returns the slot error with guidance to remove a paired device or wait
for a new allowance period. An exhausted background renewal leaves the current
auto-managed account eligible because the saved peers depend on that identity.

### 12.6 Session identity and naming

Managed session names must:

- contain 1 to 64 ASCII characters;
- start with a lowercase letter or digit;
- contain only lowercase letters, digits, `.`, `_`, and `-`; and
- not be `.` or `..`.

At startup the initial session is:

1. `ephemeral` for explicit mnemonic mode;
2. explicit `--session`, resolved by full username, username base, or a promoted
   session's original name;
3. `default` for explicit `--account`; or
4. the network's remembered `current-session`.

`--session workbench` and `/session workbench` select the most recently created
local session whose username base is `workbench`. Creation order uses the stored
account creation timestamp, not the numerical alias. `workbench.07` can be newer
than `workbench.42`; `--session workbench.42` selects that exact session. A missing
numbered username is an error and does not create another account. Base matches
compare the complete username base, so `workbench` does not select
`workbenchtest.42`. If the newest matches have equal creation timestamps, the
caller must select an exact username. With no saved base or alias match, a base
name supplies the base for a new account.

A missing or stale `current-session` resolves against the provisioned sessions
rather than falling back to `default`: exactly one session holding an account
store is selected, and several are refused with their names so `--session` can
choose. A session directory without an account store is not provisioned and is
never selected this way.

`default` is a compatibility/bootstrap session. Once an auto-managed signer is
known, the public and durable session name becomes its Lite username and its
directory becomes `<username>_signing_host`. The bootstrap name is not
user-selectable and is omitted from session completion and listing.

### 12.7 Session inspection, switching, and clearing

`/session` reports:

- `ephemeral` or the profile name;
- `<not provisioned>` or the known Lite username; and
- `<none>` or the filesystem path.

When a managed session has no connected user, startup and bare `/session` add
an actionable transcript notice directing the user to `/session <name>`.

`/session --list` includes the network directories ending in `_signing_host`.
The active session is marked with `*`.

`/session <name>` restores or provisions the target before replacing the current
runtime. Selecting the current name only returns immediately when its signer is
ready; an unfinished session retries setup. A saved account binding selects that
exact account, including imported accounts without usernames. Ready local
accounts are activated from disk without repeating network onboarding.

1. validate and create its provisional profile;
2. restore its bound or cached signer, or provision it if needed;
3. promote it to the resolved username directory;
4. load its remembered script and storage;
5. build and activate the replacement runtime;
6. persist `current-session`;
7. stop every responder for the old session;
8. swap the runtime;
9. disconnect product WebSockets using the old runtime;
10. update status and completion; and
11. restore every paired host saved for the target session.

If activation fails, the previous `current-session` pointer is restored and the
old in-memory runtime remains active. Files created while preparing the target
may remain.

`/session --mnemonic "<phrase>"` is an import-only flow:

1. parse and normalize the BIP-39 phrase;
2. derive the RFC-0022 `uid.<tld>` identity;
3. read its optional full or Lite dotNS username from Asset Hub;
4. when no dotNS mirror exists, search the identity backend's assigned username
   records for the derived candidate account;
5. confirm membership in a People or LitePeople ring;
6. use that username as the session name, or derive a deterministic
   `imported-<key fingerprint>` name when neither source has a username;
7. build and activate a replacement runtime off-side;
8. persist the mnemonic as the named `imported` account and atomically write
   the session's account binding plus its username when present;
9. persist `current-session`;
10. stop every responder for the old session;
11. swap runtimes and disconnect old product connections; and
12. restore every paired host saved for the imported session.

The backend fallback uses authenticated, cursor-paginated, prefix searches
because the backend has no account-indexed read route. It only accepts an
`ASSIGNED` row whose candidate account equals the derived identity and it never
calls a registration route. A username missing from both dotNS and the backend
does not prevent local activation; the connected session simply has no primary
username for `account.getUserId()`. A mnemonic without ring membership on the
selected network is rejected and the old in-memory runtime remains active. The
phrase is not written locally until the replacement runtime activates
successfully.

`/session --clear <name>` removes the durable name shown by `/session --list`,
its identity directory, and the matching network account cached in the
compatibility account store. `/session --clear-all` removes every such session,
the network's signing-host bootstrap state, and every compatibility account
record for that network. It does not remove pairing-host state, another
network's records, externally referenced scripts, or on-chain usernames.

The interactive UI describes the data loss and uses the existing `[y/N]`
approval. `exec` executes these explicit one-shot commands without another
flag or prompt. Clearing an inactive named session updates completion and keeps
the current runtime active. Clearing the active session or all sessions first
ends the command loop, aborts the frame server, drops the runtime, and only then
deletes the data; the signing host exits afterward. Session clearing is
unavailable in explicit-mnemonic mode.

## 13. Persistence

### 13.1 Current layout

The layout may contain compatibility paths as well as identity-owned paths:

```text
<base-path>/v2/
  accounts.json
  accounts.json.lock
  log-level

  <network>/
    signing-host/
      current-session
      session.json                    # default/bootstrap metadata, when used
      paired-hosts.json               # default/bootstrap paired hosts, when used
      paired-hosts.json.lock
      core-storage.json               # default/bootstrap core state
      scripts/
      storage/
        default/
          <product-file>.json

    pairing-host/
      current-user
      session.json                    # bootstrap script metadata, when used
      core-storage.json               # bootstrap auth/core state
      scripts/
      storage/
        <product-file>.json

    <username>_signing_host/
      accounts.json
      accounts.json.lock
      session.json
      paired-hosts.json
      paired-hosts.json.lock
      core-storage.json
      scripts/
      storage/
        <product-file>.json

    <username>_pairing_host/
      session.json
      core-storage.json
      scripts/
      storage/
        <product-file>.json
```

An explicit mnemonic has no account profile, but its signing runtime uses the
default/bootstrap signing storage path for core and product state. It does not
read or write that path's paired-host store.

### 13.2 Pairing-user storage switching

Before the first resolved user, pairing state uses
`<network>/pairing-host`. On connection:

- storage identity prefers the Lite username and falls back to the full
  username;
- display identity prefers the full username and falls back to the Lite
  username;
- the target is `<username>_pairing_host`; and
- `pairing-host/current-user` is updated atomically.

For the first bootstrap migration, in-memory bootstrap core and product values
move into the resolved user's directory.

When switching from one resolved user to another, only transient authentication
keys move:

- `AuthSession`;
- `PairingDeviceIdentity`; and
- `LastProcessedPairingStatement`.

The previous user's product KV and other core state remain in that user's
directory. The new user's existing product/core state is loaded.

Remembered pairing scripts are scoped to whichever bootstrap/user directory is
active. Resolving another user loads that directory's `session.json`.

### 13.3 Session metadata

`session.json` is version `1` and may contain:

```json
{
  "version": 1,
  "user_id": "alice.dot",
  "last_script": "script-....ts"
}
```

Scratch scripts use a single portable filename and must resolve inside the
session's `scripts/` directory. Explicit external scripts use an absolute path.
Invalid multi-component relative values are rejected. Missing scripts are
treated as not remembered.

### 13.4 Paired-host store

`paired-hosts.json` is version `1`. It contains a list of versioned peer records.
Each record contains:

- the statement account ID used as its unique key;
- the public encryption key needed to resume SSO; and
- sanitized host name, host version, icon, platform type, and platform version
  metadata when the proposal supplied them.

Records are returned and written in stable statement-account order. `/pair`
uses read-modify-write upsert semantics, so it updates the matching statement
account without replacing unrelated peers. Removal deletes exactly one matching
statement account. Both operations hold an exclusive
`paired-hosts.json.lock`. Changes are written to a process-specific temporary
file and renamed over the store.

### 13.5 Product storage

Each normalized product has one file:

```text
storage/<slug>--<sha256(product-id)>.json
```

The slug:

- retains ASCII letters, digits, `.`, and `-`;
- replaces other characters with `-`;
- is limited to 48 characters;
- trims leading/trailing `.` and `-`; and
- falls back to `product`.

The full SHA-256 digest prevents slug collisions.

The version `1` JSON document is:

```json
{
  "version": 1,
  "productId": "example.dot",
  "values": {
    "raw-product-key": "hex-encoded-value"
  }
}
```

The core has already removed its product namespace before the CLI stores the
raw key. Identity and host role are isolated by the parent directory.

Noncanonical product filenames, unsupported versions, invalid ids, and invalid
hex values are ignored with warnings.

### 13.6 Core storage

`core-storage.json` is a versionless JSON object whose keys and values are hex:

```json
{
  "values": {
    "<SCALE-encoded CoreStorageKey hex>": "<value hex>"
  }
}
```

Core state includes auth sessions, pairing bootstrap material, permission
state, and other role-owned runtime data.

### 13.7 Account store

`accounts.json` is version `1` and stores records containing:

- local name;
- network id;
- plaintext BIP-39 mnemonic;
- final Lite username;
- RFC-0022 `uid.<tld>` index-0 public key and address;
- creation timestamp;
- attested state; and
- exhausted Statement Store periods.

Account mutations hold an exclusive `accounts.json.lock`. Secret-file writes
use a temporary file, flush, atomic rename, and `0600` permissions on Unix.
The lock file can be created during a read-only cached-signer lookup.

### 13.8 Write and corruption behavior

Product and core storage writes:

- create parent directories;
- write a process/id-specific temporary file;
- flush file data;
- atomically rename;
- and sync the parent directory on Unix.

Session metadata, paired-host records, and current-user/session pointers use
temporary-file rename but do not apply the account file's explicit secret
permissions. The saved log level also uses temporary-file rename.

Malformed account, session, or paired-host JSON is a startup or command error
when that data is loaded. Malformed core JSON is warned about and loaded as
empty. Malformed product files are warned about and skipped. A malformed saved
log level produces a warning and falls back to the selected override or `info`.

There is no session-wide process lock. Account and paired-host mutations use
separate lock files, but simultaneous processes can still race on session,
core, product, and current selection files.

## 14. Network and transport

### 14.1 Network presets

`--network` selects one of two presets. `paseo-next-v2` is the default. Every
preset is a test network; the account store keeps BIP-39 entropy for
disposable test identities only.

Auto-account onboarding (§12.3) needs an identity backend that records the
lite username on the dotNS gateway. Each preset points at its matching dotSpark
identity backend, and the CLI reports the complete backend response when
registration fails.

#### `paseo-next-v2`

| Purpose | Value |
| --- | --- |
| Identity backend | `https://identity.dotspark.app/api/v1` |
| People RPC | `wss://paseo-people-next-system-rpc.polkadot.io` |
| People genesis | `0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48` |
| Bulletin RPC | `wss://paseo-bulletin-next-rpc.polkadot.io` |
| Bulletin genesis | `0x8cfe6717dc4becfda2e13c488a1e2061ff2dfee96e7d031157f72d36716c0a22` |
| Asset Hub RPC | `wss://paseo-asset-hub-next-rpc.polkadot.io` |
| Asset Hub genesis | `0x4349b00e54897e21196fd331015fc5be0f14e118beb0375ed2bb1793737bb57a` |

#### `previewnet`

The network that front-runs `paseo-next-v2`: it carries the runtime that reaches
nextv2 later, and it is where products with previewnet descriptors do their
on-chain testing.

| Purpose | Value |
| --- | --- |
| Identity backend | `https://identity-previewnet.dotspark.app/api/v1` |
| People RPC | `wss://previewnet.substrate.dev/people` |
| People genesis | `0x55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9` |
| Bulletin RPC | `wss://previewnet.substrate.dev/bulletin` |
| Bulletin genesis | `0xa081192b90c1f6a3f8e9ce7b2a8246f41af805c66456c84e05fd97c2b3502425` |
| Asset Hub RPC | `wss://previewnet.substrate.dev/asset-hub` |
| Asset Hub genesis | `0xbac97e23fc8f4bccae72a98f8aeb2bcab20bf755862304e4b46ad6473456e896` |

Sessions are per network (`SessionCatalog::new` keys on the preset id), so a
signer provisioned on one preset is not visible from the other. Two presets means
two identities on one machine, which is deliberate: the lite username and the
statement-store allowance are per chain.

The backend's username routes are bearer-gated; the CLI mints the access token
itself through the backend's `auth/challenges` → `auth/token` handshake
(§12.3), or takes one from `HOST_CLI_IDENTITY_BACKEND_TOKEN`, so auto-managed
account creation works here.

There are no public endpoint override flags. `HOST_CLI_IDENTITY_BACKEND_BASE`
replaces only the identity backend base URL (§21).

Every role the preset serves — People, Bulletin and Asset Hub — is always routed,
because host internals require all three: statement-store traffic addressed to the
People genesis, preimage submission, and PGAS claims plus dotNS username reads
respectively. The SSO sentinel is
a separate case — it is an unmapped genesis and reaches People through the fallback
below, not through People's own route. `E2E_LIVE_CHAIN=1` only widens routing to endpoints the
preset carries without serving them as a role, of which neither preset has any.

The all-zero SSO sentinel and every genesis hash not present in the active
route map fall back to the People RPC.

A rustls ring crypto provider is installed at process startup for `wss://`
connections.

### 14.2 Product-frame WebSocket

The listener uses plain `ws://`. Any `SocketAddr` accepted by the OS can be
bound, but a TCP frame connection is accepted only when its actual peer IP is
loopback. A browser WebSocket handshake must also carry an `Origin` whose host
is `localhost`, a loopback IPv4 address, or a loopback IPv6 address. Malformed
and non-loopback origins are rejected. Unix-socket connections and loopback TCP
clients without `Origin` are treated as local non-browser clients.

For a TCP listener, `GET /bootstrap.js` on the same port returns the development
bridge and shared container as a plain HTTP JavaScript response with `no-store` and connection-close
headers. Other HTTP paths return 404. The bridge embeds the endpoint that was
actually bound, so `--frame-listen` and its generated WebSocket URL remain
consistent. The HTTP response does not grant cross-origin access; browser frame
access is enforced during the later WebSocket handshake.

The browser SDK and sandbox permission checks share one WebSocket and its
`ProductRuntime`, as `/script` does. The shared SDK transport owns the socket
and uses `host:` request IDs for public calls and internal authorization methods.
The compatibility MessagePort cannot send or receive frames using those IDs.
Generated internal methods and their shared JavaScript dependencies are frozen
before product code runs; public SDK methods remain mutable. The CLI remains a local
development tool; `/script` retains its Bun/Node capabilities.
Each page load, reconnect or script run opens a fresh connection with independent
temporary permissions.

Each accepted WebSocket:

- snapshots the current normalized product;
- creates one `ProductRuntime`;
- forwards each incoming binary message as one protocol frame;
- also forwards a text message's UTF-8 bytes as a protocol frame;
- sends each emitted runtime frame as one binary message;
- disposes the runtime on close/error/reset; and
- closes on product selection or signing-runtime replacement.

Product WebSocket connections do not survive `/product` or signing-session
switches.

The frame writer uses an unbounded channel. The accept loop retries listener
accept errors. Each connection runs independently on the Tokio worker pool;
the shared service-trait contract requires dispatch futures to be `Send`.

### 14.3 Chain JSON-RPC

Every chain connection opens a fresh WebSocket. Outgoing requests use an
unbounded channel. Incoming text frames and UTF-8 binary frames enter a
1,024-message broadcast buffer.

The first response receiver is created before the reader task, preventing a
fast initial RPC response from being lost. A lagged subscriber drops missed
responses and emits a warning. `close()` prevents further sends; background
socket tasks end when their channels or sockets close.

## 15. TrUAPI capability surface

The direct and paired compatibility reports currently expose the same method
surface.

| Service | Implemented behavior |
| --- | --- |
| Account | Connection status, product accounts, aliases, proofs, empty legacy-account list, user id, and login. |
| Chain | chainHead-v1 follow/header/body/storage/call/unpin/continue/stop, chain spec queries, transaction broadcast/stop. |
| Entropy | Product-scoped deterministic entropy from the active account/session. |
| Local Storage | Persistent product-scoped read, write, and clear. |
| Notifications | In-process immediate/scheduled delivery and cancellation with transcript events. |
| Permissions | Device and remote permission approval through the CLI policy. |
| Preimage | Real Bulletin submission/lookup path plus bounded in-core read-after-write cache. |
| Resource Allocation | Real host-managed allocation: Bulletin long-term storage over SSO, and an Asset Hub PGAS claim for `SmartContractAllowance`. |
| Signing | Product and legacy transaction construction, raw signing, and payload signing. |
| Statement Store | Real subscribe, proof, authorized proof, and submit over People. |
| System | Handshake, feature query, and no-op navigation. |
| Theme | One `Dark` subscription value. |
| Chat | Typed unavailable/empty-subscription behavior. |
| Coin Payment | Typed unsupported/interrupted-subscription behavior. |
| Payment | Typed unsupported/interrupted-subscription behavior. |

### 15.1 Exact reported methods

Implemented-success methods in the checked-in direct and paired battery
reports:

- `Account/connection_status_subscribe`
- `Account/get_account`
- `Account/get_account_alias`
- `Account/create_account_proof`
- `Account/get_legacy_accounts`
- `Account/get_user_id`
- `Account/request_login`
- `Chain/follow_head_subscribe`
- `Chain/get_head_header`
- `Chain/get_head_body`
- `Chain/get_head_storage`
- `Chain/call_head`
- `Chain/unpin_head`
- `Chain/continue_head`
- `Chain/stop_head_operation`
- `Chain/get_spec_genesis_hash`
- `Chain/get_spec_chain_name`
- `Chain/get_spec_properties`
- `Chain/broadcast_transaction`
- `Chain/stop_transaction`
- `Entropy/derive`
- `Local Storage/read`
- `Local Storage/write`
- `Local Storage/clear`
- `Notifications/send_push_notification`
- `Notifications/cancel_push_notification`
- `Permissions/request_device_permission`
- `Permissions/request_remote_permission`
- `Permissions/authorize_remote_permission`
- `Preimage/lookup_subscribe`
- `Preimage/submit`
- `Resource Allocation/request`
- `Signing/create_transaction`
- `Signing/create_transaction_with_legacy_account`
- `Signing/sign_raw_with_legacy_account`
- `Signing/sign_payload_with_legacy_account`
- `Signing/sign_raw`
- `Signing/sign_payload`
- `Statement Store/subscribe`
- `Statement Store/create_proof`
- `Statement Store/submit`
- `Statement Store/create_proof_authorized`
- `System/handshake`
- `System/feature_supported`
- `System/navigate_to`
- `Theme/subscribe`

Deliberately unavailable methods:

- all five product-initiated Chat methods, because the CLI installs no
  `ChatPlatform` for the `App` execution kind these reports run under;
- the product-initiated `Renderer/action_subscribe`, because
  `renderer_access_for` grants product Renderer access only to a `Worker`
  execution and these reports run the CLI as `App`; the host-initiated
  `Renderer` render subscription is also unused because the CLI draws no
  product-rendered bodies;
- all nine Coin Payment methods, which answer `CallError::Unsupported`; and
- all four Payment methods, which answer typed `Unknown` domain errors.

A successful `System/feature_supported` call resolves the queried chain against
the host's chain set, the same set `Chain/get_chain_info` answers from, so it
returns `true` for the preset's People, Bulletin and Asset Hub genesis hashes and
`false` for anything else. Success means the method is wired, not that every feature
is present.

### 15.2 Platform-specific semantics

Navigation logs the requested URL at debug level and returns success; it does
not open a browser.

Notification ids start at `1` per process. A future `scheduledAt` value is
treated as Unix milliseconds and delivered by a Tokio timer. At most 64
notifications may be pending. Cancelling an unknown/already-delivered id is a
successful no-op.

A preimage lookup the core cannot answer from its own cache goes to the
network's Bulletin node, by CID over `bitswap_v1_get`. A miss is asked again
every 6 s until the blob lands; a request the node can never answer ends the
lookup with an error. The core also owns the real Bulletin client and a
separate 16 MiB insertion-ordered preimage bridge for read-after-write
behavior.

Statement Store keeps up to 64 accepted statements in an insertion-ordered
read-after-write bridge until the remote subscription reports them.

## 16. Entropy, accounts, and authority policy

The signing host uses raw BIP-39 entropy. Product entropy applies three
BLAKE2b-256 layers:

1. keyed root source using domain `product-entropy-derivation`;
2. a product layer keyed by the hash of the normalized product id; and
3. a caller-context layer keyed by 1 to 32 context bytes.

During pairing, only the pre-hashed root entropy source is shared with the
pairing host. Therefore paired and direct hosts derive the same value only
when all three are the same:

- signing account/root entropy;
- normalized product id; and
- context bytes.

Product accounts, entropy, and Ring-VRF contexts reject cross-product use
unless the core policy and any required user confirmation allow it.

Same-product Ring-VRF alias/proof requests follow the signing-runtime policy
and do not add a CLI-only prompt.

## 17. Approvals and security behavior

### 17.1 Prompt policy

Without `--auto-accept`, platform approval is deny-by-default.

The interactive signing host accepts `/approval`, `/approval manual`, and
`/approval automatic`. The bare command reports the current mode. A setter
changes every future platform confirmation and remains active when a session
or identity switch rebuilds the runtime. The setting is process-local and is
not written to session state. A new process derives its initial policy from
`--auto-accept` again.

In the TUI it uses the approval card described in section 9. In plain mode:

- stdin must be a TTY;
- action confirmations offer `[y] Approve` and `[n] Reject`;
- permissions offer `[o] Allow once`, `[a] Allow always` and `[n] Deny`;
- full words (`yes`, `once`, `always`, `no`) are also accepted; and
- EOF, invalid input, or non-TTY stdin rejects.

Approval summaries exist for:

- SCALE payload signing;
- raw-data signing;
- transaction construction;
- account alias derivation;
- account proof creation;
- identity disclosure;
- resource allocation;
- preimage submission;
- cross-product account access;
- device permission; and
- remote permission.

Remote permission summaries identify the domains or capability requested. Domain
grants cover all ports, including local services. Device prompts identify the
capability. Permission reviews preserve Allow once without writing a permanent
grant; ordinary action confirmations remain Boolean.

Raw signing payloads are hidden from approval summaries. Proof summaries show
only product and message length.

### 17.2 Auto-accept

`--auto-accept` approves actions and returns Allow always for permissions. It emits:

```text
✓ Approved <action> automatically
  <redacted summary>
```

It does not bypass product-id validation or core authorization rules.

### 17.3 Sensitive state and output

The dedicated pairing-link event necessarily prints the complete deeplink in
plain mode and shows it in the live TUI. Treat captured output as sensitive.
Transcript copies and submitted-command dividers redact it.

Mnemonics are never intentionally printed, but auto-managed mnemonics are
stored in plaintext `accounts.json`. That file is local test secret material,
not production custody.

`debug` and especially `trace` can include decoded product payloads and
transport metadata. Do not publish trace logs from sensitive test accounts
without review.

## 18. Events, output, and logging

### 18.1 Human output

v0.1 has one human output format. There is no JSON or JSONL mode.

Outside the TUI:

- lifecycle and command results use stdout;
- tracing and many diagnostics use stderr through the tracing writer;
- Clap and explicit invocation errors use stderr; and
- script stdio is inherited in top-level `--script` mode.

Representative output:

```text
• Listening for product frames
  ws+unix:/tmp/truapi-host-…/frames.sock
✓ Paired
✓ Signing host ready
◌ Script running
✓ Script finished
```

The same `SystemEvent` presentation code supplies plain and TUI wording.
Passing `--frame-listen 127.0.0.1:0` instead reports
`ws://127.0.0.1:<allocated-port>`.

### 18.2 Lifecycle events

The CLI exposes events for:

- frame listener readiness;
- signing-host readiness;
- exhausted signer-account rotation;
- responder start/stop/failure;
- product connection reset after session/profile replacement;
- personhood ring discovery;
- wallet and device allowance preparation/results;
- notification scheduling/delivery/cancellation;
- pairing link/authentication/connection/disconnection/failure;
- script start/exit;
- session status/create/switch;
- log-level change; and
- transcript copy.

### 18.3 SSO transcript

SSO summaries use a dedicated tracing layer and remain visible at every log
level. Request and response events with the same Statement Store request id
update one transcript row.

When available, rows contain:

- humanized request name;
- statement request id;
- remote/response message id;
- protocol outcome;
- elapsed milliseconds; and
- encoded error reason.

Fallback SSO summary text is still shown when structured fields are absent.

### 18.4 Log filtering

Startup selects an explicit `--log-level` or `TRUAPI_HOST_LOG` value first,
then the level saved by `/log`, then `info`. A valid `RUST_LOG` replaces that
scoped startup filter, and its trimmed value replaces the selected level in the
status bar. `/log` replaces the startup filter and status value with the
selected level, then saves it under `<base-path>/v2` for later launches.

Without `RUST_LOG`, the selected CLI level applies to:

- `truapi`
- `truapi_host`

Other targets remain at `warn`.

The following noisy targets are always hidden from the ordinary CLI log layer:

- `truapi::sso_transcript` (handled by its dedicated layer);
- `rustls` and its children; and
- `tungstenite::protocol` and its children.

Logging ANSI is disabled before messages enter the transcript.

## 19. Diagnostic commands

### 19.1 `identity-check`

```text
truapi-host identity-check \
  --mnemonic <BIP-39> \
  [--network paseo-next-v2]
```

The command derives and queries two accounts:

- root; and
- RFC-0022 `//product//uid.<tld>/index_bytes(0)`, `<tld>` being the selected
  network's dotNS TLD (`paseo` for `paseo-next-v2`, `testnet` for `previewnet`).

For each it prints one of:

```text
IDENTITY_FOUND path=<path> account=<ss58> username=<name>
IDENTITY_NONE path=<path> account=<ss58>
IDENTITY_ERROR path=<path> account=<ss58> error=<reason>
```

Per-path RPC errors are printed and do not make the command itself fail.
Mnemonic parsing failures do fail the command. The mnemonic is not persisted.

### 19.2 `register-name`

```text
truapi-host register-name \
  --mnemonic <BIP-39> \
  --label <base-label> \
  [--network paseo-next-v2] \
  [--link-lite <name.NN> | --chat-key <65-byte-hex>]
```

Registers `label` as the full-person username of the mnemonic's RFC-0022
`uid.<tld>` identity account, through `DotnsGateway.register_name` on Asset Hub.
The account must be a recognized full person: its ring-VRF key must be built
into a People-collection ring root on People, and Asset Hub's
`members-subscriber` must already hold that root revision (the command waits for
it). The People ring, its members and its root revision are read at one
finalized People block.

`--link-lite` links the new name to a dotted lite username, inheriting that
name's chat key; without it and without `--chat-key`, the account's own lite
username (from dotNS) is linked. `--chat-key` registers standalone with the
given ECDH key. The two are mutually exclusive.

The transaction is a General (v5) extrinsic authorized by the `AsDotnsGateway`
extension: `RegisterFullName { proof, ring_index, revision, signature }`, where
`signature` is the account's sr25519 signature over the inherited-implication
digest and `proof` the ring-VRF membership proof, built for the `revision` read
above. `RestrictOrigins` carries `true`. Success prints:

```text
REGISTER_SUBMITTED label=<label> block=<hash>
REGISTER_ALIAS alias=0x<alias>
REGISTER_CONFIRMED label=<label> full_username=<name>
```

`REGISTER_ALIAS` is printed once `DotnsGateway.AccountAlias` records the
account, `REGISTER_CONFIRMED` once the dotNS contracts return the name (or
`<pending>`). Before signing: the label must be unminted on `DotnsRegistrar` (a
pending reservation is not a mint and stays claimable); the account must not
already hold a `DotnsGateway.AccountAlias`; a linked lite username must be
owned by the account per `DotnsGateway.LiteLabelOwner`; and runtimes whose
`RegisterFullName` shape differs are rejected.

### 19.3 `alloc-check`

```text
truapi-host alloc-check \
  --mnemonic <BIP-39> \
  [--network paseo-next-v2] \
  [--target <32-byte-hex>] \
  [--lookback 8] \
  [--submit]
```

It prints:

- runtime spec version;
- transaction version;
- genesis hash;
- per personhood collection, the derived bandersnatch member key and that
  collection's current ring index;
- per collection, matching ring details, or a single onboarding-pending line when
  no collection includes the member key;
- current allowance period;
- target account;
- per collection, the free or already-allocated slot, a scan error, or a note that
  the chain does not offer that collection; and
- the submission result, naming the collection the slot was taken in, when
  requested.

Each collection is a separate alias space with its own slot budget, so the scan
reports one table per collection rather than one combined table.

Without `--target`, the target is all zeroes and the command is scan-only.
`--submit` requires an explicit 32-byte target. `0x` is optional on target
hex.

Submission uses the shared metadata-driven `set_statement_store_account`
implementation, pooled across every collection whose membership the signer can
prove. An allocation already held in any collection is reused. When every
collection is full it replaces the globally oldest replaceable slot across all of
them; on-demand allocation for a product reports exhaustion instead.

## 20. Exit status and shutdown

| Status | Meaning |
| --- | --- |
| `0` | Successful command, diagnostic, or script. |
| `1` | General runtime/state/network error, invalid product at runtime construction, or failed `exec` script. |
| `2` | Clap/explicit invocation error, non-TTY interactive use, malformed slash command passed to `exec`, or runner connection timeout in top-level script mode. |
| child status | Top-level pairing/signing `--script` preserves a normal Bun exit status. |
| wrapped status | `dev` preserves a normally exiting direct launcher's status after descendant cleanup. |
| `130` | `dev` received SIGINT or SIGTERM and completed wrapped-command cleanup. |

Interactive command errors do not terminate the host. They finalize running
activities, display the error, and return to the command bar.

Dropping a `SigningHostSession` stops all of its background responders. Leaving
the TUI restores the cursor, bracketed-paste mode, alternate screen, and raw mode.
The frame accept task is aborted when its owning command body completes.

`dev` owns explicit SIGINT/SIGTERM orchestration and the wrapped-command
lifecycle described in section 6.6. Other commands have no global signal
controller. Interactive Ctrl-C is handled as a terminal key; external process
termination follows normal operating-system behavior.

Top-level `--script` uses `std::process::exit` after the frame-server scope has
ended. This preserves the child status but bypasses later Rust destructors.

## 21. Environment variable reference

| Variable | Scope |
| --- | --- |
| `TRUAPI_HOST_LOG` | Per-process `--log-level` override. |
| `TRUAPI_DEBUGGER_URL` | Default `--debugger` dial. |
| `RUST_LOG` | Full startup tracing filter. |
| `TRUAPI_HOST_BASE_PATH` | Default `--base-path`. |
| `TRUAPI_HOST_NO_UPDATE` | Any value disables the self-update check. |
| `TRUAPI_HOST_INSTALL_DIR` | Version store for a managed install. Read by the installer; the binary derives it from its own path. |
| `TRUAPI_HOST_BIN_DIR` | Directory the installer puts the `PATH` symlink in. |
| `TRUAPI_HOST_VERSION` | Version the installer installs, instead of the current stable one. |
| `TRUAPI_HOST_RELEASE_BASE_URL` | Release host for the installer and the updater, for mirrors and tests. |
| `HOST_CLI_SIGNER_MNEMONIC` | Mnemonic for `dev`, `signing-host`, `identity-check`, `register-name`, `alloc-check` and `pgas-check` when `--mnemonic` is omitted. |
| `HOST_CLI_IDENTITY_BACKEND_BASE` | Identity backend base URL override, including `/api/v1`, for instance a local backend. Chain endpoints stay on the preset. |
| `HOST_CLI_IDENTITY_BACKEND_TOKEN` | Bearer token for the identity backend's username routes. For registration its subject must be the candidate `uid.<tld>` account. Unset, the CLI mints one itself through the backend's `auth/challenges` → `auth/token` sr25519 handshake with that identity key. |
| `HOST_CLI_DOTNS_POP_CONTROLLER` | `DotnsPopController` H160 override, skipping on-chain discovery (`DotnsGateway.DispatcherAddress`, used directly when `protocolRegistry()` answers on it, otherwise resolved through `TARGET()`). Only needed where discovery fails. The controller is `0xCC932348606cc1f3318cADeC5A5Cd2CA447f8a4b` on paseo-next-v2 and previewnet; `DEPLOYMENTS.md` in paritytech/dotns is the authority per network. |
| `XDG_STATE_HOME` | Preferred default state parent. |
| `HOME` | Fallback default state parent. |
| `VISUAL` | Preferred script editor. |
| `EDITOR` | Fallback script editor. |
| `TRUAPI_HOST_RUNNER` | Override `js/runner.ts`. |
| `E2E_LIVE_CHAIN` | Value `1` widens routing to endpoints the preset does not serve as a role; no effect on either preset. |
| `NO_COLOR` | Disable CLI semantic colors and battery reporter color. |
| `COLORFGBG` | Infer TUI background color. |
| `COLORTERM` | Select true-color TUI rendering. |
| `FORCE_COLOR` | Force battery reporter color in non-TTY output. |
| `TRUAPI_BATTERY_REPORT_PATH` | Override battery report destination. |
| `TRUAPI_APPROVALS_LOG` | Append one line per decided confirmation to this file. |

## 22. Current v0.1 operational constraints

These are part of the as-built specification:

- only the `paseo-next-v2` and `previewnet` test presets are selectable; there is no mainnet preset;
- product scripts require Bun; installed releases include a self-contained runner;
- there is no structured/JSON output mode;
- there is no `--version`;
- there is no script timeout option;
- commands other than `dev` have no global signal-aware graceful-shutdown controller;
- onboarding can wait for the fixed identity/ring polling windows;
- session/core/product state has no inter-process mutation lock;
- corrupt core storage is treated as empty after a warning;
- non-loopback product listeners can bind but reject every TCP frame peer;
- product text WebSocket frames are accepted as protocol bytes;
- product-frame and chain outbound queues are unbounded;
- unknown chain genesis hashes fall back to People;
- interactive child ANSI styling is stripped rather than parsed; and
- pairing and signing state are local plaintext test state.

## 23. Verification contract

The implementation is covered by:

- CLI unit tests for parsing, completion, TUI rendering, storage, accounts,
  products, sessions, approvals, and platform behavior;
- process-boundary tests for help, non-TTY rejection, product reporting,
  session restore, cached signer activation, and bare-script safety;
- `truapi` runtime, protocol, cryptographic vector, and integration
  tests;
- shared container, script-runner/Bun diagnosis and packaged-runtime tests;
- paired and direct `battery.ts` runs, both driven by `scripts/battery.sh`; and
- checked-in compatibility reports:
  - `explorer/diagnosis-reports/spa/pairing-host-cli.md`
  - `explorer/diagnosis-reports/spa/signing-host-cli.md`

The reports currently have identical method results apart from their title:

- 65 rows: 52 succeeding methods and 13 failing ones;
- 9 Coin Payment methods, which answer `CallError::Unsupported`; and
- 4 Payment methods, which answer a typed `Unknown` domain error.

The Chat surface does not appear: it requires a `Chat` execution, and these are
SPA reports. Two caveats on the checked-in reports: the enumeration in 15.1
lists 45 methods and predates later additions to the surface, and only the
signing-host report carries measured `Unsupported` Coin Payment details — the
pairing-host report still records the older `HostFailure` shape, because the
pairing phase needs a personhood ring member before it runs any method. It
refreshes on the next `make e2e-pairing-cli` run from such a signer.

Recommended local verification after CLI changes:

```sh
cargo +$(cat nightly-toolchain) fmt --check
cargo clippy -p truapi-host-cli --all-targets -- -D warnings
cargo test -p truapi-host-cli
cargo test -p truapi-host-cli --bin truapi-host caller_configuration_cannot_execute_before_the_sandbox -- --include-ignored
bun test js/container/src scripts/cli-runner-package.test.ts
git diff --check
```

The launcher regression requires Bun and is ignored by ordinary `cargo test`.
CI runs it explicitly with the same command above.
