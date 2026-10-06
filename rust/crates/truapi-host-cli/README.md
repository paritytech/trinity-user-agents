# truapi-host-cli

Headless TrUAPI hosts for local end-to-end testing, built on `truapi`.
They replace the external signing-bot service: two CLI processes take the two
host-spec §B roles and pair over the **real People-chain statement store** (the
same node an iOS/web client uses), so tests run against a real signer with no
Novasama-operated dependency.

See [SPEC.md](SPEC.md) for the complete as-built v0.1 behavior and engineering
contract.

Either host can be driven by a **product script** you write: a JS/TS file that
receives a global `truapi` (the `@parity/truapi` client, scoped to a product id)
and calls it like any product would. With `--script`, the CLI runs the script
and exits with its status. Without `--script`, both roles open a full-screen
terminal UI when stdin and stdout are TTYs.

One binary, `truapi-host`:

| Command | Role |
| --- | --- |
| `pairing-host` | Seedless host: serves product frames, emits pairing deeplinks, and can run product scripts. |
| `signing-host` | Wallet-local host: owns signer identity, can run product scripts, decodes copied pairing QR images or accepts deeplinks, registers statement allowance on-chain, signs. |
| `dev` | Run a local development product with the shared container loaded by a script tag. |
| `identity-check` | Probe the root and the network's `uid.<tld>` identity account for a registered username (read from the dotNS contracts on Asset Hub). |
| `register-name` | Register a full-person username via `DotnsGateway.register_name` on Asset Hub, linked to a lite username or standalone with a chat key. |
| `alloc-check` | Diagnose (or `--submit`) on-chain statement-store allowance: ring membership, chosen slot, and the `set_statement_store_account` extrinsic. On a full period it prints each occupied slot's age and which one would be replaced. |
| `pgas-check` | Diagnose (or `--submit`) an Asset Hub PGAS allowance claim: ring membership on People, whether Asset Hub has imported that ring revision, the day's first unclaimed slot, and the `Pgas.claim_pgas` extrinsic. |

The repository's `make e2e-dotli` target builds this binary and runs the
dotli/playground Diagnosis suite with a non-interactive signing-host responder.
It verifies the initial pairing, remote signing, host sign-out, and
same-account reconnect without the external signer-bot service.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash
truapi-host signing-host
```

Prebuilt binaries exist for `aarch64-apple-darwin`,
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. The Linux
binaries are statically linked, so they run on any distribution. The installer
puts each version in `$XDG_DATA_HOME/truapi-host/versions/<version>/` and
symlinks `~/.local/bin/truapi-host` through a `current` link, so an update only
moves that one link.

| Variable | Effect |
| --- | --- |
| `TRUAPI_HOST_VERSION` | Install this version instead of the current stable one. |
| `TRUAPI_HOST_INSTALL_DIR` | Version store, default `$XDG_DATA_HOME/truapi-host`. |
| `TRUAPI_HOST_BIN_DIR` | Directory the `PATH` symlink goes in, default `~/.local/bin`. |

Product scripts (`--script`, `/script`) work from an installed binary: the
archive ships a self-contained `runner.js` with the `@parity/truapi` client and
shared web API permission checks bundled in, plus `script-types.d.ts` for the
globals it injects. You still need `bun` on `PATH`, since it executes the runner
and your script. The development container is shipped separately as
`sandbox-assets/container.js`.

Product frames use a private, per-process WebSocket-over-Unix-domain-socket by
default, so starting either host does not reserve a TCP port. Pass
`--frame-listen 127.0.0.1:0` to expose an ordinary loopback WebSocket instead;
this is required for browser clients, which cannot open filesystem sockets.

### Staying current

A managed install checks for a new release at most once every four hours,
alongside whatever command you ran rather than delaying it, and installs it into
the version store. A command that finishes first waits for the download, so even
a one-shot run lands the update; it prints a line while downloading. The running
process is never replaced underneath itself: the new version takes effect the
next time you start `truapi-host`, and the CLI says so when one is waiting.
Every archive is checked against its published SHA-256 before it is unpacked.

```bash
truapi-host update            # check and install now
truapi-host --version         # what is running
TRUAPI_HOST_NO_UPDATE=1 ...   # never check
```

Binaries that the installer did not put in place — a `cargo install` copy, a
source build, a distro package — are detected and never modified. A local build
says so on every run and prints the install command, since it otherwise looks
identical to a managed install that is quietly up to date.

The two install routes shadow each other depending on `PATH` order, so each one
clears the other: installing removes a `cargo install` copy, and
`make headless install` removes a prebuilt install first. To remove a prebuilt
install without replacing it:

```bash
curl -fsSL https://raw.githubusercontent.com/paritytech/trinity-user-agents/main/scripts/truapi-host-installer.sh | bash -s -- --uninstall
```

`make e2e-cli-update` exercises the whole chain locally: it packages the binary,
serves a fake release over loopback, installs it with the real installer, and
updates it. Nothing contacts GitHub.

### State directory

Reserved identities derive under `uid.paseo` / `peopl.paseo` on
`paseo-next-v2`, and `uid.testnet` / `peopl.testnet` on `previewnet`.
Managed host state lives under `<base-path>/v2`, including accounts,
sessions, pairings, core and product storage, and log
preferences. The CLI appends `v2` to both the default base path and a path set
through `--base-path` or `TRUAPI_HOST_BASE_PATH`. For example,
`--base-path ./truapi-host-paseo` uses `./truapi-host-paseo/v2`.
New script projects live separately under `<base-path>/scripts` so clearing
host sessions does not delete them.

The CLI leaves previous host state outside `v2` untouched and unused, and starts
normal onboarding automatically. There is no state migration. Pair devices
again; sign out first on any paired host that still uses an old identity.
Existing `.dot` personhood membership does not transfer to the new keys.

### Building from source

A source build resolves the product-script runner from the checkout, so it also
needs the generated `@parity/truapi` sources. (An installed release ships its
own bundled runner and does not.) Dev also needs the container bundle generated
by `make headless` or `make cli-runner`. To build and install the CLI yourself:

```bash
make headless install  # build dependencies and install truapi-host once
truapi-host signing-host
```

Requires stable Rust, nightly Rust with rustfmt, Node.js 22 or newer, and Bun. The target installs missing workspace
build tools and regenerates Rust and TypeScript sources on every run, including after updating an existing checkout.

### Raw proof contexts (development only)

A product can bind a ring-VRF proof to 32 bytes of its choosing instead of a
product-namespaced context by calling `development_createAccountProof` from
`@parity/truapi`; the signing host honours it as is. Yet to be removed before a
production release.

### Browser products

`truapi-host dev` is one command for "run this product as if it were inside a
host". It starts a signing host on loopback, waits for the signer, then runs the
wrapped development command with the host already live:

```bash
truapi-host dev -- yarn dev
```

The product reaches it through a development-only tag, which the host serves
itself:

```jsx
{process.env.NODE_ENV === "development" && (
  <script src="http://127.0.0.1:9955/bootstrap.js" />
)}
```

That script installs the SDK bridge and the shared `js/container` sandbox
synchronously. Keep it before application scripts, without `async` or `defer`.
Updated SDKs use the injected `__HOST_API_CLIENT__` across reconnects. Older SDKs
can still start through the `__HOST_API_PORT__` adapter but require a page reload
after a disconnect. SDK calls and sandbox permission checks share one host
WebSocket and its temporary permissions.
`/script` uses the same fetch and WebSocket permission checks in Bun, and the
XHR wrapper when that API is available. Dev keeps automatic approvals, and the
app server handles assets and hot reload. `--app-port` names the development
server's port when it is not 3000, and the product id defaults to that origin, so the host and the
product cannot disagree about who they are. `--port` changes the bridge and
frame port, but the development-only tag must change to the same value.

On Unix the development command starts in a process group owned by the CLI. If
the direct launcher exits, or the CLI receives SIGINT or SIGTERM, the CLI sends
SIGTERM to the complete group, waits up to five seconds, then sends SIGKILL if
anything remains. This catches the intermediate processes that package
managers put in front of the actual dev server. A natural launcher exit keeps
its exit status; an interrupted run exits with status 130. Non-Unix platforms
stop and reap the direct child.

A host that should outlive the development server, or one whose confirmations
you want to approve by hand in the terminal UI, is the same host started
directly with your development server run separately. Every host mode serves the
bridge script, so the product tag is unchanged:

```bash
truapi-host signing-host --frame-listen 127.0.0.1:9955 --product-id my-product.dot
```

For an API connection without installing the container, a product can call
the SDK directly before anything else touches the client:

```ts
import { connectWebSocketHost } from "@parity/truapi/sandbox";

connectWebSocketHost("ws://127.0.0.1:9955");
```

TCP frame connections are accepted only from loopback peers. Browser WebSocket
requests must also carry a `localhost` or loopback-IP `Origin`. WebSocket is not
subject to CORS, so without both checks another page or remote non-browser
client could drive a host that auto-approves confirmations. Unix-socket clients
and loopback TCP clients that send no `Origin` are treated as local processes.

The product is then detected as hosted and holds the real product account for
its own `.dot` name, so signing, statements, entropy, permissions and storage
all take their production code paths with no phone involved. `--product-id` is
not optional: the host derives the product account from it and refuses to _sign_
for any other product id, and a mismatch only surfaces later, as a
`PermissionDenied` on the first signature.

Two players on one machine means two hosts, each with its own session and port
(`--session bobsmith --frame-listen 127.0.0.1:9956`), and a second product instance
pointed at the second port. Sessions isolate the signer, the storage and the
permissions.

The signing host opens an interactive terminal where you can type `/pair` and
press Ctrl-V, use the terminal's paste shortcut, or drop an image file. You can
also provide an image file or deeplink with `/pair <value>`, run `/script`, or
use `/help` to discover the available commands. It uses `--mnemonic` /
`HOST_CLI_SIGNER_MNEMONIC` if set.
Otherwise it auto-selects or creates a stored account under `<base-path>/v2`
(default `$XDG_STATE_HOME/truapi-host/v2` or `~/.local/state/truapi-host/v2`),
attests it through the identity backend, waits for ring readiness, and rotates
when the current account exhausts Statement Store slots and no saved pairing
depends on its identity. A full period replaces the oldest slot past the runtime's
replacement cooldown, so rotation only happens when no slot is replaceable.

### Interactive terminal UI

In a TTY, both hosts open the same scrollable transcript above a single command
bar. Host lifecycle events, tracing logs, every incoming SSO request, script
stdout/stderr, commands, and approval prompts all use that transcript, so
background output cannot overwrite input. The status bar shows the active log
value. On
`signing-host`, `--deeplink URL` opens the UI and starts the pairing response
after initialization.

Commands always start with `/`:

| Command | Result |
| --- | --- |
| `/pair` | Wait for a pairing QR image from Ctrl-V, terminal paste, or drag-and-drop (signing host). |
| `/pair <image-path>` | Decode a pairing QR from a PNG, JPEG, or WebP image (signing host). |
| `/pair <url>` | Validate and answer a `polkadotapp://pair?...` deeplink (signing host). |
| `/devices` or `/devices --list` | List every paired device saved for the active signing-host session. |
| `/devices --remove <statement-account-id>` | Disconnect and remove one paired device by its 32-byte statement account ID. |
| `/devices --remove <statement-account-id> --force` | Attempt to disconnect one paired device, then remove its local pairing even if notification fails. |
| `/approval` | Show whether signing-host confirmations are manual or automatic. |
| `/approval manual` | Prompt for every future signing-host confirmation. |
| `/approval automatic` | Approve every future signing-host confirmation automatically. |
| `/script` | Edit and run the remembered script, creating a project when needed. |
| `/script <path>` | Remember and run an existing JS/TS product script through the public frame endpoint. |
| `/script --run` | Rerun the remembered script without opening the editor. |
| `/script --edit` | Edit the remembered script without running it. |
| `/script --new [directory]` | Create a project in a new directory, then edit and run it. |
| `/login` | Start pairing for the selected product, show its QR code, and copy its deeplink to the clipboard. |
| `/logout` | Disconnect the pairing host and discard its old pairing keypair. |
| `/log <level>` | Save tracing as `error`, `warn`, `info`, `debug`, or `trace`, and apply it now. |
| `/product` | Show the currently selected product. |
| `/product <id>` | Switch the product used by future scripts and frame connections. |
| `/session` | Show the current session name, path, and user id (signing host). |
| `/session <name>` | Restore or create an isolated signing-host session, retrying unfinished setup even for the current name. |
| `/session --mnemonic "<phrase>"` | Import an existing signer as a durable session. |
| `/session --list` | List user sessions for the current network. |
| `/session --clear <name>` | Permanently clear one signing-host session. |
| `/session --clear-all` | Permanently clear every signing-host session for the current network. |
| `/help` | Show commands and keyboard shortcuts. |
| `/clear` | Clear the visible transcript. |
| `/copy` | Copy the retained transcript to the system clipboard. |
| `/quit` | Shut down cleanly. |

### Pasting a pairing QR image

Copy the QR image shown by the app, run `/pair` in an interactive signing host,
then press Ctrl-V or use the terminal's normal paste shortcut, such as Command-V
on macOS. Both forms read image pixels from the operating-system clipboard, so
the image is not converted to terminal text and the flow works inside tmux.

While `/pair` is waiting, you can also drag an image file into the terminal. If
the terminal inserts the path without submitting it, press Enter. Raw, quoted,
shell-escaped, and `file://` paths are accepted. `/pair <image-path>` remains
available for direct file input. PNG, JPEG, and WebP files are supported.
One-shot `exec` mode accepts an image path or deeplink but cannot wait for a
clipboard paste or drop.

Clipboard and file images are decoded in memory and are never written to a
temporary file. The decoder accepts regular and light-on-dark QR codes, including
the circular finder styling used by Polkadot apps. It distinguishes an image
without a QR code, an unrelated QR code, and multiple pairing codes. Copy another
image and paste again after a clipboard error, or press Ctrl-C to cancel.

Images are limited to 8192 pixels per edge and 24 million pixels. Image files are
also limited to 64 MiB. The decoded value must be exactly one valid
`polkadotapp://pair?handshake=...` proposal before it reaches the existing
pairing responder.

Typing `/` opens autocomplete. Up/Down selects a completion; with the menu
closed it navigates process-local command history. Tab inserts a completion,
and `/script` completes filesystem paths. Enter submits `/session <name>` as
typed; use Tab first to insert a suggested exact session name.
Mnemonic characters are masked while
typing and mnemonic commands are never retained in command history or the
transcript. Ctrl-U/Ctrl-D scroll by half a
viewport, End restores auto-follow, Esc closes autocomplete, and Ctrl-C clears
input, cancels a running command, or exits when idle. Deeplinks are deliberately
not persisted in history across processes.

On `pairing-host`, `/logout` cancels an in-flight pairing, disconnects the
current signing host, and removes the old pairing identity. The next product
login request or operator `/login` generates a new keypair and emits a fresh
link that can be answered by another signing host. `/login` uses the current
`/product` selection, copies the generated deeplink to the system clipboard,
and remains interactive while the TUI renders a scannable QR code and pairing
progress. Product-driven login requests show the same QR code without changing
clipboard contents. The raw link remains in the transcript, and a terminal
that cannot fit the whole QR falls back to that link instead of wrapping or
clipping it. A clipboard or QR-rendering failure is reported without cancelling
pairing.
Logout does not clear product storage, scripts, or the selected product.

Both `pairing-host` and `signing-host` use the same interactive UI and command
bar. It uses a quiet, command-centered transcript: submitted
commands title full-width dividers, script stdout keeps the terminal's normal
foreground, stderr has a small error gutter, and lifecycle work updates
sentence-case status rows in place. A compact
`TrUAPI <role> host · 👤 <name> · 🌐 <network> · 📦 <product>` status sits
below the writing bar. Long product names are ellipsized, while session and log
level stay out of that bar. A borderless, subtly backgrounded composer anchors
autocomplete and the `›` prompt while keeping the native cursor after the
input. When the input is empty, command guidance appears there as a placeholder
instead of occupying status space. Set `NO_COLOR=1` to remove semantic colors
and the surface fill without losing spacing, status symbols, or wording.

Non-interactive `--script` and `exec` runs use the same sentence-case event
copy and status symbols without the full-screen chrome. This keeps captured
logs readable while pairing URLs remain directly extractable by automation.
`/copy` copies readable transcript text without UI chrome or complete pairing
links. Captured script output is plain text: the host strips terminal control
sequences before adding child output to the transcript. Raw ANSI styling such
as bold is therefore not rendered in the full-screen UI.

Bare `/script` reopens the last script recorded for the active session,
including a path previously selected with `/script <path>`. If that file is
missing or the session has no script yet, it creates a Bun TypeScript project
under `<base-path>/scripts/`, outside the versioned session data. Projects
survive session clearing, including projects created with `--mnemonic`.
Each contains `script.ts`, host declarations, `package.json`, and `tsconfig.json`.
The starter runs the Product SDK quickstart: create an app, connect wallet
accounts, and write and read local storage. The runner supplies the SDK's host
connection automatically. The first open installs SDK and editor dependencies with
Bun. Successful setup saves a lockfile; later opens reuse
the installation without a network request. Setup errors or cancellation keep
the project so you can fix the problem and retry `/script`.

`/script --new my examples` creates another project at that relative path;
the directory must not already exist. `/script --edit` only edits, while
`/script --run` reruns without opening an editor. Paths may contain spaces.
Use `/script -- --run` to select a file literally named `--run`.

Missing npm imports are installed into Bun's cache at runtime. For editor types
and locked versions, add packages locally with `bun add`, plus separate types
when needed (for example, `bun add -d @types/lodash`). Check types with
`bun run typecheck`. The project's `package.json` entry
`"truapiHost": { "script": "script.ts" }` identifies its root, which is also
its execution directory. This remains true when copied or selected through
an explicit path. Update that entry when renaming the main script. Ordinary
existing npm/pnpm/Bun projects keep their dependencies, lockfiles, and the
CLI's original working directory; the host does not install into them.

The TUI temporarily yields the terminal to `$VISUAL`, then `$EDITOR`, or
`vi` when neither is set. After the editor exits successfully, the TUI is
restored and the saved script runs through the public frame endpoint. Editor
settings containing arguments, such as `EDITOR='code --wait'`, are supported.
Configure a waiting editor command: an editor process that returns immediately
also lets execution start immediately. Editor failure preserves the script
without running it. Managed projects open the editor from their project root,
so editor commands such as `bun run typecheck` use that project's dependencies.

New projects request the latest Product SDK, TypeScript, and Bun editor types.
The SDK is installed locally so its dependencies and editor types are available.
Choose your own version with `bun add @parity/product-sdk@<version>`, or remove
it with `bun remove @parity/product-sdk` when your script does not use it.
The project lockfile records installed versions. Host updates preserve existing
project dependencies.

Managed sessions isolate signer accounts, product/core storage, and permissions.
Once a signer identity is known, its public session name is the Lite username
and its files live under
`<base-path>/v2/<network>/<username>_signing_host`. Provisional named sessions
are promoted to that user-owned root. `--session workbench` and `/session
workbench` select the most recently created local session whose username base
is `workbench`. Creation order comes from the stored account timestamp, not the
numerical alias: `workbench.07` can be newer than `workbench.42`. Use the full
username, such as `--session workbench.42`, to select that exact session.
If that numbered username is not saved locally, selection fails without creating
another account.
Promoted sessions also retain their original names as aliases. The selected
username is remembered per network but is not repeated in the status bar as a
separate session field.
`default` remains only as a compatibility/bootstrap location until a username
is resolved. It is hidden from session completion and listing and cannot be
selected with `/session default`. User session names contain lowercase ASCII
letters, digits, `.`, `_`, or `-`; they cannot be paths. Switching prepares the
target while the old session remains active, then stops all responders for the
old session, resets product WebSocket connections, and restores every paired
device saved for the target session. Reload browser dev pages to connect to the
new runtime.

`/session --mnemonic "<phrase>"` brings an already-onboarded account into the
session catalog. The host derives its `uid.<tld>` identity, reads any existing
full or Lite username from dotNS, falls back to the identity backend's assigned
username records when no dotNS mirror exists, and confirms its People or
LitePeople ring membership. This lookup is read-only and never registers a new
username. On success, the resolved identity username becomes the session name;
only an account absent from both sources uses a deterministic
`imported-<key fingerprint>` name and connects the account without username
metadata. The mnemonic is written to that session's `0600` account store and
the exact account record is restored on restart and when switching sessions.
An invalid phrase or an account without ring membership on the selected network
leaves the current runtime active. A username-less session can sign and connect, but
`account.getUserId()` cannot return a primary username until one source has a
record. Use the interactive command when practical: putting the same command in
`exec` also puts the phrase in your shell's arguments/history.

New auto-managed accounts use the session name as their Lite username prefix;
digits and separators are omitted. The derived base must contain at least six
lowercase ASCII letters. A new `/session foo` fails immediately with a too-short
username-base error, before network onboarding. For example, session `pgtest`
requests the base `pgtest` when no matching local session exists. `--session`
selects this base; there is no separate username-prefix option. `default` retains
the `headless` base. Existing saved accounts and aliases can still be selected
by their original names.
The backend assigns a numerical alias to the unchanged base. An exhausted base
reports an error; the CLI does not generate alternative bases or random suffixes.
The base is saved with the pending account, so retrying unfinished setup keeps
the same identity.
`--reserved-username <label>` additionally reserves a
full-person base name on dotNS for a newly created account, to be claimed later
with `register-name`; the CLI refuses labels the registrar has already minted.
The selected username and last script reference are cached in `session.json`
inside the displayed session path. Legacy session-local scripts use a portable
filename; managed projects and explicit scripts use an absolute path. On restart
or session switching, an already-provisioned local signer is activated from disk without an
identity-backend or ring-membership round trip, and bare `/script` restores that
session's editor context. A session with no signer yet reports
`<not provisioned>` and the transcript prompts the user to run
`/session <name>`. Naming a session creates and connects its user if needed,
including when that name is already selected but setup is unfinished. Selecting
an already-connected session keeps its signer and product connections.
Inspecting with bare `/session` never starts network onboarding.

Each managed session stores all of its paired hosts in `paired-hosts.json`.
Mutations are serialized through `paired-hosts.json.lock`. `/pair` inserts or
updates the host selected by its statement account ID only after the encrypted
handshake response is submitted, and leaves every other responder running.
Interactive mode and `--serve` restore responders for all saved hosts at
startup. Transient responder failures and ended subscriptions are retried with
backoff. A remote `Disconnected` message removes only that peer's saved pairing
and responder.

Handled SSO request IDs are stored per signing identity and paired host. The
signing host records a request before executing it, so restarting or retrying a
responder acknowledges an unexpired duplicate without repeating its side
effects.
Missing or overly distant request expiry is bounded to the seven-day SSO
statement lifetime.

`/devices` and `/devices --list` show the saved statement account IDs in stable
order with available host and platform metadata. Interactive
`/devices --remove <statement-account-id>` asks for confirmation. The same
command through `exec` is an explicit one-shot removal and runs without another
prompt. Removal first submits `Disconnected` to the selected remote host. Only
after the statement store accepts it does it stop that responder, remove the
saved pairing, and stop its allowance renewal. A submission failure preserves all
local pairing state. The other saved pairings and the signing identity are
unchanged. For recovery when notification cannot be submitted, append
`--force`. The command still attempts notification first, but warns and
continues with local cleanup if that attempt fails or times out after 30 seconds.
Submission does not wait for the remote host to acknowledge receipt. The remote
host may continue to show stale connected state, but it cannot reach a responder
on this signing host.

`/session --clear <name>` permanently deletes that session's local signer
keys, session-local scripts, core/product storage, and permissions. Persistent
script projects under `<base-path>/scripts` are preserved. `/session --clear-all`
does the same for every signing-host session on the current network, including
the network's signing-host bootstrap state, while preserving other networks and
pairing-host state. Neither command deregisters an on-chain username. The interactive UI
asks for `[y/N]` confirmation. `exec` treats the explicit one-shot command as
confirmation and runs it immediately. Clearing an inactive named session keeps
the host running; clearing the active session or all sessions stops the signing
host after its runtime and product connections have shut down.

Without `--session`, the session is the one the network's `current-session`
pointer names. When that pointer is missing or names a session that no longer
exists, the provisioned sessions decide instead: a base path holding exactly one
session with an account store reselects it, and a base path holding several is
refused with their names so `--session` can choose, rather than provisioning
another identity. A session directory without an account store is not an
identity and is never reselected. This makes one `--base-path` per caller the
supported way to hold a reusable signer.

Restore or create a signer at interactive startup with:

```bash
truapi-host signing-host --session aliceuser
```

Provisioning runs in the terminal UI, where progress and errors are visible.
If it fails or is cancelled, retry with `/session aliceuser`. Starting without
`--session` restores a cached signer but waits for an operation that needs one
before creating an account. `--serve` and script execution also ensure a signer
is ready.

In `exec` mode, `--session` selects the session for the slash command. Inspection,
listing, help, and clearing do not provision accounts. Use
`signing-host --session aliceuser exec '/session aliceuser'` to explicitly restore or
create the signer without opening the UI.

`--session` cannot be combined with `--account` or `--mnemonic`. A host
started with an explicit mnemonic reports an `ephemeral` session and does not
allow runtime switching.

Only one operational command runs at once, but SSO traffic and approvals keep
flowing while it runs. Without a TTY, use one-shot `exec` mode (parent options
come first):

```bash
truapi-host signing-host exec '/session'
truapi-host signing-host exec '/session --clear alice.01'
truapi-host signing-host exec '/session --clear-all'
truapi-host signing-host --auto-accept exec '/script ./js/scripts/ring-vrf-smoke.ts'
truapi-host signing-host exec '/pair polkadotapp://pair?handshake=...'
truapi-host signing-host --session alice.01 exec '/devices'
truapi-host signing-host --session alice.01 exec '/devices --list'
truapi-host signing-host --session alice.01 exec '/devices --remove 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
truapi-host signing-host --session alice.01 exec '/devices --remove 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef --force'
```

`exec` does not enable raw mode or emit terminal controls. Command results go
to stdout, diagnostics go to stderr, and the process exits when the command
finishes. Starting `signing-host` without `--script` or `exec` while either
stdin or stdout is not a TTY is an invocation error unless `--serve` is used.
The existing `--script` one-shot mode remains supported. Neither one-shot mode
restores saved responders. Only a deeplink supplied for that run is answered,
and a managed session still saves that pairing for a later interactive or
`--serve` run. By contrast, an explicit startup mnemonic has no managed session,
so its pairing runs only for the current process and `/devices` is unavailable.

## Writing a product script

A product script is top-level JavaScript or TypeScript (an ES module) run by
Bun. It can import npm dependencies available beside the script or in a parent
project. Before importing it, the runner installs the same fetch and WebSocket
permission wrappers used by `dev`, plus the XHR wrapper when that API exists.
Rust decides Allow once, Allow always or Deny; domain grants cover all ports.
Public SDK calls and permission checks share one product execution.

Scripts retain Bun/Node filesystem, environment, subprocess and import access.
Both CLI flows use ordinary SDK authorization requests to test permissions on
patched web APIs. Product code can deliberately bypass these development checks,
including through native networking in Bun. Native hosts retain their separate
authorization protection.

Start the host with the product id used by the SDK quickstart:

```bash
truapi-host pairing-host --product-id my-app.dot
```

Use `/login` to connect a signing host, then `/script` to open the starter:

```ts
import { createApp } from "@parity/product-sdk";

const app = await createApp({
  name: "my-app",
  logLevel: "info",
});

// Connect to host-provided accounts.
const { accounts } = await app.wallet.connect();

console.log("Connected accounts:", accounts);

// Persist a value. Namespaced under the app name in host storage.
await app.localStorage.set("lastVisit", new Date().toISOString());

const lastVisit = await app.localStorage.get("lastVisit");
console.log("Last visit:", lastVisit);
```

The SDK resolves `my-app` to the wallet product `my-app.dot`, so keep the app
name and host product id aligned. The quickstart uses default cloud storage
on Paseo, the CLI's default network. When signed out, wallet connection can
return an empty account list. Existing projects use the same ordinary SDK
imports and their installed dependencies. Check a managed project with
`bun run typecheck`.

Await all work, including subscription completion: the script process exits
when its module and optional default function finish. After losing the host
connection, rerun the script.

For direct TrUAPI calls, the runner also injects three globals:

- **`truapi`** is the `@parity/truapi` client connected to the host and
  scoped to the host's `--product-id`. Call `truapi.account.requestLogin(...)`,
  `truapi.signing.signRaw(...)`, `truapi.localStorage.write(...)`, etc.
- **`host`** provides `host.productId` and `host.productAccount(index?)`.
  Product accounts use the host's `--product-id`; a mismatched id
  fails signing with `PermissionDenied`.
- **`assert`** throws when its condition is false, using any following values
  as the error message.

Declare those globals from the project's generated types when using them in
TypeScript. The declarations stay local to each script:

```ts
import type {
  TrUApiClient,
  HostContext,
  ScriptAssert,
} from "./script.types.d.ts";

declare const truapi: TrUApiClient;
declare const host: HostContext;
declare const assert: ScriptAssert;
```

Write calls at the top level and `throw` (or reject) to fail the run:

```ts
const login = await truapi.account.requestLogin({ reason: undefined });
if (
  !login.isOk() ||
  (login.value !== "Success" && login.value !== "AlreadyConnected")
)
  throw new Error("login failed");

const res = await truapi.signing.signRaw({
  account: host.productAccount(),
  payload: { tag: "Bytes", value: { bytes: "0xdeadbeef" } },
});
res.match(
  (v) => console.log("signature", v.signature),
  (e) => {
    throw new Error(JSON.stringify(e));
  },
);
```

For product-account signing through the SDK, use the `host` and `assert`
declarations above with:

```ts
import { getAccountsProvider } from "@parity/product-sdk/host";

const accounts = await getAccountsProvider();
assert(accounts, "Host accounts API unavailable");
const account = await accounts.getProductAccount(host.productId);
if (account.isErr()) {
  throw new Error("Product account unavailable", { cause: account.error });
}
const signer = accounts.getProductAccountSigner(account.value);
const signature = await signer.signBytes(new TextEncoder().encode("hello"));
console.log("signature", signature);
```

This subscription example awaits the first locale value and unsubscribes before
the script exits:

```ts
import {
  getLocaleProvider,
  type HostSubscription,
} from "@parity/product-sdk/host";

const locales = await getLocaleProvider();
assert(locales, "Host locale API unavailable");
let subscription: HostSubscription | undefined;
let removeInterrupt: (() => void) | undefined;
try {
  await new Promise<void>((resolve, reject) => {
    subscription = locales.subscribeLocale((locale) => {
      console.log("language", locale.languageTag);
      resolve();
    });
    removeInterrupt = subscription.onInterrupt((reason) => {
      reject(new Error("Locale subscription interrupted", { cause: reason }));
    });
  });
} finally {
  removeInterrupt?.();
  subscription?.unsubscribe();
}
```

`--product-id` (a dotNS name ending in `.dot`, `.paseo` or `.testnet`, or a
`localhost` identifier; default
`headless-playground.dot`) sets the initial product. `/product <id>` changes it
for the lifetime of the process. Switching disconnects active product
WebSockets. Reload browser dev pages to use the new product context. The
network, pairing relationship, signing-host session, and wallet identity stay active.
Product-owned storage, permissions, and derived product accounts are scoped by
the selected id, so the newly selected product sees its own state. The next
`/script` also receives the new id through `host.productId`.

Pairing-host state follows the same identity rule under
`<base-path>/v2/<network>/<username>_pairing_host`. Before the first identity is
known it uses the small `<network>/pairing-host` bootstrap; connecting moves
that bootstrap data to the first resolved user. After `/logout`, connecting
as a different user swaps to that user's KV/core namespace instead of carrying
the previous user's product data forward.

Product-local KV is persisted independently under each identity root as
`storage/<safe-product-slug>--<hash>.json`. Each document records its normalized
product id and raw product keys. Product and core JSON writes use a flushed
temporary file and atomic rename.

Scripts under `js/scripts/` include:

- `battery.ts` — the generated full-surface gate. It discovers every method
  from the same code-generated example manifest as the playground Diagnosis,
  attempts all examples (including APIs the browser diagnosis classifies as
  intentionally unsupported), prints test-reporter rows with timings and clean
  failure details, writes the browser-shaped result matrix to
  the role-specific report under `explorer/diagnosis-reports/spa/`, and exits
  nonzero if any example fails. A paired run writes `pairing-host-cli.md`; a
  direct signing-host run writes `signing-host-cli.md`. Override the artifact
  path with `TRUAPI_BATTERY_REPORT_PATH`.

  On top of the generated examples it runs one hand-written
  `Resource Allocation/auto_signing_e2e` case: allocate `AutoSigning`, then
  prove through the hosts' consulted-approval transcript
  (`TRUAPI_APPROVALS_LOG`, exported per phase by `scripts/battery.sh`) that the
  calls the grant covers — `sign_vrf`, `sign_raw`, `sign_payload` and
  `create_transaction` — run for the granting product without a confirmation
  prompt.

  `scripts/battery.sh` at the repo root is the supported entry point. It
  prepares the codegen output and playground dependencies the battery imports,
  builds the host from source, and produces both reports in one invocation: the
  direct signing-host phase, then the paired phase, where it starts a pairing
  host, reads the `polkadotapp://pair?...` link out of its transcript, and
  answers it with a second signing host using the same product id and forwarded
  host flags so the battery can complete:

  ```bash
  scripts/battery.sh                    # both phases
  scripts/battery.sh --signing-host     # direct phase only
  scripts/battery.sh --pairing-host     # paired phase only
  make e2e-signing-cli                  # direct phase only
  make e2e-pairing-cli                  # paired phase only
  make e2e-chat-cli                     # chat phase only
  scripts/battery.sh --pocket-host      # Pocket phase only
  make e2e-pocket-cli                   # Pocket phase only
  scripts/battery.sh --release          # release binary
  scripts/battery.sh -- --network foo   # arguments after `--` go to every host process
  ```

  `BATTERY_PHASE_TIMEOUT` (default 900s) bounds each phase and
  `BATTERY_PAIRING_TIMEOUT` (default 120s) bounds the wait for the pairing link.
  Per-phase host transcripts land in `target/battery/`.

  The Pocket phase runs its product as a Worker execution, because that is the
  only execution Pocket is served to. It seeds the in-memory Pocket host from
  `TRUAPI_POCKET_CARDS` (`loyalty,humanity:privileged`), so one card is
  removable and one privileged, and records every removal the host is asked for
  in `TRUAPI_POCKET_LOG`. The cases read that transcript, so a pass means the
  host and the product agree rather than resting on the product's word.

  Contacts are served on every phase, from `TRUAPI_CONTACTS`
  (`alice=0x<32-byte account>;bob=0x…`) or, unset, from a two-name development
  list so `contacts.pick` has someone to return. An empty spec is an empty list,
  which is what answers `NoContacts`. `TRUAPI_CONTACT_PICK` names which contact
  the picker offers; the approval surface asks about it like any other action,
  so a headless run approves it and an interactive one does not.

  The paired phase gives its pairing host a throwaway `--base-path` under
  `target/battery/pairing-host-state`, so it performs a real handshake on every
  run. A pairing host that restores an earlier session reports
  `AlreadyConnected` and then fails every remote example, because the signing
  host that session was paired with is no longer running. The signing host keeps
  the default base path and reuses its attested account.

  To drive the paired topology by hand instead, start the pairing host and
  answer its emitted link from a second terminal:

  ```bash
  # Terminal 1
  cargo run -p truapi-host-cli -- pairing-host \
    --product-id truapi-playground.dot \
    --script rust/crates/truapi-host-cli/js/scripts/battery.ts \
    --auto-accept

  # Terminal 2
  cargo run -p truapi-host-cli -- signing-host \
    --deeplink '<pairing link>' \
    --auto-accept
  ```

- `device-removal-disconnect.ts`: verifies `Connected` followed by `Disconnected`.
  Run it through `e2e/device-removal-disconnect.sh`, which pairs isolated hosts,
  removes the device interactively, and checks cleared pairing auth storage and
  an empty signing-host device list. Run `make codegen` once in a fresh checkout,
  build `truapi-host-cli`, then run the shell script.

- `whoami.ts` — calls `getUserId` and prints `WHOAMI <primary username>`; this
  remains available as an explicit `/script <path>` example.
- `signing-smoke.ts` — a focused product-account signing check.
- `smart-contract-allowance-smoke.ts` — requests a PGAS allowance for product
  account index 0. Reports `Allocated` against `paseo-next-v2`; a host that serves
  no Asset Hub role reports `NotAvailable` rather than failing. The direct path asks
  for `Increase`, so each run submits a real claim and spends one of the day's slots
  rather than noticing the account is already funded: repeat runs within a day can
  exhaust them and then fail for that reason rather than a regression. The host logs
  the real cause, which the wire value flattens to `NotAvailable`.
- `ring-vrf-smoke.ts` — registers and lists an explicit RFC-0024 key, derives
  its alias, verifies a fresh non-member key returns `NotMember` for a proof,
  and exercises direct ring-VRF signing.
- `preimage-smoke.ts` — a focused Bulletin preimage flow check.

The generated examples are baked to the `truapi-playground.dot` product. With
live routing enabled, `Chain/stop_transaction` uses host-owned operation ids and
treats already-finished provider operations as stopped. `Preimage/*` also uses
the real Bulletin Next chain and asks the signing host to claim People-chain
long-term storage before returning the product-scoped Bulletin allowance key.
It needs the playground's deps (`cd playground && yarn install --frozen-lockfile`;
bun does not resolve the `link:` dependency on `@parity/truapi`). Repeated live
runs can exhaust the signer's per-period Statement Store or Bulletin allocation
slots. Statement Store registration replaces the oldest slot whose replacement
cooldown has elapsed, so exhaustion needs every slot to be within that
cooldown; the signing host rotates auto-managed signer accounts if that
happens.

## Confirmations

Both hosts take `--auto-accept`. Without it, confirmations a web/iOS host would
show as a modal (sign requests, permission prompts, and cross-product Ring-VRF
requests) are rendered prominently in the signing-host transcript. Actions use
`y` to approve and `n` to reject. Permissions use `o` for Allow once, `a` for
Allow always and `n` for Deny. Typed answers plus Enter also work. Approval
cards summarize and redact signing payloads rather than dumping debug objects.
The current command draft is restored afterward; Esc rejects. Concurrent
approvals are serialized. Plain mode offers the same choices when stdin is a
TTY; non-TTY stdin rejects instead of hanging. Same-product Ring-VRF requests do not
prompt, matching the iOS signing host. Pass `--auto-accept` for unattended
runs; every auto-approved decision is still printed.

The interactive signing host can inspect or change its running policy with
`/approval`, `/approval manual`, and `/approval automatic`. A change applies to
future confirmations and survives session switches within that process. It is
not saved, so the next process starts from `--auto-accept` again. These commands
are unavailable on the pairing host and in one-shot `exec` mode.

## Logging

Use the global `--log-level` option (`error`, `warn`, `info`, `debug`, or
`trace`) before or after the subcommand, or `/log <level>` in the terminal UI.
`/log` saves the level under `<base-path>/v2`, so pairing and signing hosts restore
it after restart. A one-off `--log-level` or `TRUAPI_HOST_LOG` value overrides
the saved level for that process without changing it; otherwise the fallback is
`info`.
Every decoded inbound SSO request and every published response is visible
regardless of the selected level. Stable response entries include the request
name, statement and remote message ids, protocol outcome, and elapsed time;
encoded protocol errors include their reason. Response-publication failures
are shown separately. `debug` adds decoded request/response summaries and
`trace` adds complete payload and transport metadata. Undecodable requests are
warnings with the available identifiers so protocol-version mismatches can be
diagnosed.

```bash
truapi-host signing-host --log-level trace --deeplink '<deeplink>' --auto-accept
```

Debug and trace output may contain product signing payloads. `RUST_LOG` takes
precedence at startup and remains available for module-specific filters, except
that the noisy `rustls` and `tungstenite::protocol` tracing targets are always
excluded from CLI log output. The status bar continues to show the selected CLI
level when `RUST_LOG` is absent; otherwise it shows the exact `RUST_LOG` value.
`/log` replaces the startup filter with the selected level. Without `RUST_LOG`,
`--log-level` and `/log` apply to TrUAPI targets while other third-party
dependencies remain at `warn`.

## Wire debugger

`--debugger <URL>` streams every product frame this host sends or receives to a
[`@parity/truapi-debugger`](../../../js/packages/truapi-debugger) listening on a
loopback `ws://` address. `TRUAPI_DEBUGGER_URL` sets the same value; an explicit
flag wins over it.

```bash
# in one shell
( cd js/packages/truapi-debugger && npm run serve )   # 127.0.0.1:9231

# in another
truapi-host dev --debugger ws://127.0.0.1:9231 -- yarn dev
```

The switch is meaningful only on the commands that serve frames: `pairing-host`,
`dev` and `signing-host`. A URL that is not loopback fails startup rather than
warning, since a host that runs on without the debugger it was asked for looks,
from the debugger's side, exactly like a host nobody switched on. Starting the
debugger after the host is fine: the sink dials lazily and reconnects.

Every run says which way it went, either `Streaming wire frames to a debugger`,
naming the endpoint and the switch clap read it from, or `Wire debugger off`.
That report is the reason the flag needs no build gate: a stale exported
`TRUAPI_DEBUGGER_URL` cannot tap a session quietly.

Frames are forwarded whole and the debugger decodes all of them, so a tapped
signing host puts its payloads on that socket. Point it at a debugger you are
running yourself.

## Statement-store allowance

The real statement store enforces per-account allowance. Before pairing, the
signing host grants it on-chain exactly as a real client does: it proves its
personhood ring membership with a bandersnatch ring-VRF and submits an unsigned
General (v5) `Resources.set_statement_store_account` extrinsic for each account
that submits statements — its RFC-0022 `uid.<tld>` identity account and the
pairing host's per-pairing device key. The shared native implementation lives in
`truapi/src/runtime/statement_allowance/` (metadata-driven
signed-extension encoding, ring fetch, slot scan, ring-VRF proof, extrinsic
assembly, submit). The signing account must be an attested member of at least
one personhood collection, and may sit in an old ring, so the signing host scans
back from the current ring index (slow, one-time per pairing).

Each collection is a separate alias space with its own budget, so a signer with
full personhood has the slots returned by
`Resources.get_stmt_store_slots_per_period` in `People` on top of
`Resources.get_lite_stmt_store_slots_per_period` in `LitePeople`. These dynamic
values and the replacement cooldown are read through runtime view functions and
cached with the runtime metadata. Asset Hub budgets PGAS claims the same way,
through `Pgas.MaxClaimsPerPeriodPerPerson` and
`MaxClaimsPerPeriodPerLitePerson`, and a claim is scanned against the budget of
the collection it is proved against. A PGAS claim proves one collection rather
than pooling across both, so it is bounded by that collection's budget alone.

Registration pools across every collection the signer can prove, and a free slot
anywhere is taken before any live slot is replaced. Whether a live slot may be
replaced at all depends on the caller. The renewal pass, the pairing-time grant,
and `alloc-check --submit` may replace, and then take the globally oldest
replaceable slot across all collections. Allocation on behalf of a connecting
product may not: it reports the period as exhausted, because every entry in the
table is one of this wallet's own products and reclaiming space belongs to the
renewal pass. `alloc-check` prints both collections' member keys, ring indices and
slot tables. Auto-managed accounts are stored in
`accounts.json` under `<base-path>/v2`; mnemonics are plaintext local test secrets
and the file is written with `0600` permissions on Unix. `alloc-check` verifies
membership and can submit a test registration.

When a managed session has saved pairings, Statement Store exhaustion does not
mark its signer as exhausted or rotate to another identity. If there is no slot
for another device, pairing fails and preserves the existing identity and
pairings. Remove a paired device or wait for a new allowance period before
trying again.

## Manual use (two terminals)

```bash
make headless install

# Terminal 1 — pairing host runs a product script and prints its pairing link:
truapi-host pairing-host --product-id myapp.dot --script js/scripts/battery.ts --auto-accept

# Terminal 2 — hand the deeplink to a signing host (registers allowance, signs).
# The wallet mnemonic comes from --mnemonic / $HOST_CLI_SIGNER_MNEMONIC when set;
# otherwise the CLI auto-selects or creates an attested account.
truapi-host signing-host --deeplink '<deeplink>' --auto-accept
HOST_CLI_SIGNER_MNEMONIC="spin battle …" truapi-host signing-host --deeplink '<deeplink>' --auto-accept

# Inspect on-chain statement-store allowance for a mnemonic:
truapi-host alloc-check --mnemonic "spin battle …" --lookback 100
```

Both hosts take `--network`, either `paseo-next-v2` (default) or `previewnet`.
The network preset owns the identity backend URL, the People, Bulletin and Asset
Hub RPCs, and their genesis hashes; there is
no public `--statement-store` flag. Pick `previewnet` when a product's runtime
descriptors target previewnet, so its statements, its host chain routes and its
own chain reads all land on one network. The CLI mints the identity backend's
bearer token itself (SPEC.md §12.3). Sessions are per preset, so each network
gets its own signer identity on the same machine.
`HOST_CLI_IDENTITY_BACKEND_BASE` swaps only
the identity backend (for a local one); `HOST_CLI_IDENTITY_BACKEND_TOKEN`
supplies its bearer token instead of the CLI minting one. For username
registration, an injected token's subject must match the session's `uid.<tld>`
candidate account. The automatically minted token uses that identity; and
`HOST_CLI_DOTNS_POP_CONTROLLER` overrides on-chain `DotnsPopController`
discovery (see SPEC.md §21). Both also accept `--frame-listen <address>`
to opt into a TCP product-frame WebSocket; without it, the CLI creates and
cleans up a unique temporary Unix socket.

## Serving a dev server (one process, no terminal)

`truapi-host dev` serves the browser bootstrap. Use `--serve` when a separate
process needs the raw product-frame endpoint.

`signing-host --serve` runs the host as a background service instead of a
terminal UI, so a dev server or test harness can supervise it:

```bash
truapi-host signing-host --serve \
  --frame-listen 127.0.0.1:9955 \
  --product-id myapp.dot \
  --auto-accept
```

It needs no TTY, initialises the signer, restores responders for every paired
device saved in the selected session, and stays up until stopped. Output is one
line per event:

```
✓ Paired with headless.43
✓ Signing host ready
• Listening for product frames
  ws://127.0.0.1:9955
• Browser bridge
  http://127.0.0.1:9955/bootstrap.js
  Load it from a development-only <script> tag to run a product in a plain browser tab
• Serving product frames until stopped
  ws://127.0.0.1:9955
  Confirmations are approved automatically
```

Wait for `Serving product frames until stopped` before pointing a product at the
endpoint. That line is last in every case, and it is the only one that means
both halves are up: the frame socket accepts connections well before a signer
exists, and `Signing host ready` can arrive either side of it depending on
whether the session was cached or is being registered. A first run registers a
lite username and the statement-store allowance on-chain, which can take
minutes; it announces `Provisioning a signer` when it starts, so a supervisor
can tell that work from a stalled process. A session with no signer at all
reports `No connected user` here as it does in the terminal.

Stopping it: Ctrl-C is handled, so the host logs its own shutdown. `SIGTERM`
ends the process, which is what a supervising dev server sends.

`--auto-accept` is effectively required, because a process with no terminal has
nowhere to prompt: confirmations are denied instead, and the startup line says
so. `--serve` cannot be combined with `--script` or `exec`, which are the
one-shot modes.

## Scope / gaps

- **Chain methods** route to real `wss://` nodes from the selected `--network`.
  Every role the preset serves is routed unconditionally; `E2E_LIVE_CHAIN=1` only
  widens routing to endpoints it carries without serving. A rustls crypto provider is
  installed at startup for the TLS connections.
- **Ring-VRF product-account aliases and proofs** are implemented by the
  signing host via the `verifiable` crate (`get_account_alias` and
  `create_account_proof`).
- **`get_user_id`** resolves the signing account's username from the dotNS
  contracts on Asset Hub. Auto-managed signing accounts register fresh lite
  usernames via the identity backend (`src/attestation.rs`); first registration
  is backend-async and can take minutes (ring onboarding). `truapi-host
identity-check --mnemonic <m>` probes which derivation carries a username.
- `set_statement_store_account`, Bulletin long-term-storage, and Asset Hub PGAS
  resource allocation are implemented over SSO on native headless hosts.
- Everything else the browser host exercises passes: signing (raw, payload,
  create-transaction, and their legacy variants), statement store, entropy,
  aliases, preimage, storage, permissions, notifications, theme, system, chain
  and user id, subject to live chain availability
  and allowance-slot capacity.
