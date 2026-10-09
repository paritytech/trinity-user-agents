# Repository agent guidance

This repo is the single source of truth for the TrUAPI protocol. It vendors `dotli` as a git submodule at `hosts/dotli/`.

## Layout

```
rust/crates/
  truapi/                Rust trait + type definitions for protocol versions v0.1 and v0.2
                         (canonical), plus the runtime hosts implement (default `runtime`
                         feature); ships as WASM (browser/node); its `platform` module
                         holds the host syscall traits (storage, navigation, consent, ...)
  truapi-codegen/        rustdoc JSON → TypeScript client + Rust dispatcher
  truapi-macros/         #[wire_trait(id = N)] and #[wire(id = N)] proc-macros;
                         #[sso_service] for truapi's inter-host SSO protocol
                         One implementation module per macro; lib.rs holds entry points
  truapi-provider/       network provider backends (WebSocket RPC or smoldot light-client);
                         its `platform` module holds the chain-access traits
  truapi-verifiable/     ring-VRF operations over `verifiable`; a lazily loaded WASM module in the browser
  truapi-host-cli/       CLI pairing/signing hosts; Bun scripts share the container web API gates
js/packages/
  truapi/                  @parity/truapi TS package; generated TS lives under ignored paths
  truapi-host/            @parity/truapi-host: WASM-backed host runtime. Subpath entries:
                          `.` (shared host types), `/web` (iframe + Web
                          Worker), `/worker-runtime` (Worker entry), and the
                          test host `/testing` (createMockHost),
                          `/testing/playwright` (fixture), `/testing/server`
                          (node server), `/testing/client` (no-iframe client),
                          `/testing/dev-accounts`, `/testing/host-page`.
                          Two WASM bundles (gitignored) under dist/wasm/, built
                          via `make wasm`: `web/` is the production browser host
                          and `testing/` adds the `wasm-signing-host` and
                          `test-host` Cargo features the test host needs
  truapi-debugger/        @parity/truapi-debugger (published to npm): the debugger.
                          Owns all decoding of the wire frames the Rust host tap
                          (truapi's DebugSink) streams out, and decodes
                          every frame by default (no denylist, no reveal toggle).
                          Holds the trace, envelope-decode, and value-decode
                          engines, the shared view model + renderers, and two
                          mounts over them: server.ts (standalone WS+HTTP app on
                          127.0.0.1:9231 that hosts dial into, `npm run serve`;
                          endpoints /, /op-list, /op, /view, /channels, /stats,
                          /traces, /frame) and in-app.ts (createInAppDebugger:
                          same-page host, no server, no dial). @parity/truapi has
                          no debug seam. Where the app ultimately lives is still
                          an open decision.
js/container/              Shared TS lockdown container for native web views and CLI dev; scripts reuse its web API gates
                           `npm run build` bundles it into ios/truapi-host/Sources/TrUAPIHost/Resources/
ios/truapi-provider/       TrUAPIProvider Swift package (chain transport over UniFFI);
                           second product of the root Package.swift, released on its
                           own tag (@parity/ios-provider@<v>) via its scripts/
android/truapi-host/       truapi-host-android AAR (bindings + Kotlin shell + per-ABI
                           cdylib), published to GitHub Packages by release-android;
                           include `@parity/android-host <version>` in the `release:`
                           PR title
android/truapi-provider/   truapi-provider-android AAR; bundles the cdylib the same way,
                           so consumers need no Rust toolchain
ios/truapi-host/           TrUAPIHost Swift package over the truapi UniFFI core;
                           SPM manifest at the repo root (Package.swift), rebuild via
                           ios/truapi-host/scripts/rebuild.sh
playground/                Next.js interactive playground; deploys to the truapi-playground dotNS label
hosts/ios/                 iOS host app; resolves the core from this tree
hosts/android/             Android host app
hosts/imports.json         source repository and imported revision per host,
                           read and updated by scripts/refresh-host-import.sh
hosts/dotli/               dotli submodule
docs/                      design docs, RFCs, feature proposals
scripts/codegen.sh         regenerate the TS client from the Rust crate
scripts/battery.sh         run the generated battery against both headless CLI host roles,
                           plus the Pocket phase a Worker execution serves
scripts/bundle-size.mjs    measure the JS/WASM of the asset groups given (raw, gzip, brotli);
                           .github/actions/bundle-size takes them as `assets`, stores
                           main's snapshot and comments the comparison on PRs
scripts/refresh-host-import.sh
                           refresh a vendored host tree from its source repository
scripts/host-papp-fixtures.ts
                           print host-papp's encoding of the SSO messages the core
                           pins, from a triangle-js-sdks checkout
scripts/truapi-host-installer.sh
                           one-liner installer for the prebuilt truapi-host CLI
scripts/build-cli-runner.ts
                           bundles the CLI runner and development container
scripts/cli-runner-package.test.ts
                           verifies the installed runner and development container
.github/consumers.json     maps each released package to the repos notified by a bump issue
nightly-toolchain          the dated nightly for rustfmt, CI clippy and rustdoc JSON; CI, the
                           scripts and the Makefile all read it
.github/registry-drift-exceptions.json
                           documents intentionally unpublished npm package versions
```

### Crate + binding invariants

- `truapi` holds the canonical protocol definitions and, behind its default
  `runtime` feature, the host runtime. The protocol half builds without the
  runtime (`--no-default-features`), which is what codegen links and reads, so
  protocol modules never depend on runtime modules. Syscall traits live in the
  `platform` module, host-side runtime types in the runtime modules. The whole
  crate is one UniFFI namespace, `truapi`, so native bindings use protocol types
  directly. The published binaries keep their `truapi_server` names
  (`truapi_server.xcframework`, `dist/wasm/*/truapi_server*`).
- Treat concrete modules such as `truapi::v01` as implementation details of
  the canonical `truapi` crate and its version-conversion impls. Everywhere
  else, import concrete protocol payload and error types from `truapi::latest`.
  This includes structs reused by host-internal APIs that are not exposed to
  products; if such a type is missing, re-export it through `truapi::latest`
  rather than importing a concrete protocol version. Runtime code may use
  `truapi::versioned::*` for wire envelopes, but should unwrap them into latest
  payloads immediately.
- Inter-host SSO uses `#[sso_service]` as described
  in the [macro guide](rust/crates/truapi-macros/README.md). Keep per-variant pairing,
  dispatch, and correlation in that macro; do not add manual per-variant catalogs.
  Requests with a caller share `ProductRequest<P>` around canonical payloads;
  handler method names select request variants. Responses share `Response<P>`;
  handler signatures name their result payload
  and response variant. Transcript classification belongs in shared reply handling
  or the handler; request context carries only the call and signing session.
- Native bindings expose canonical Rust domain and protocol types directly.
  Add feature-gated UniFFI derives to those types and custom conversions for
  unsupported leaf values instead of defining parallel `Native*` mirrors.
  Boundary-specific native types are reserved for lifecycle or callback
  behavior that has no canonical value-type equivalent.
- `truapi-host-cli`'s crate version tracks `js/packages/truapi/package.json`,
  kept in sync by `scripts/sync-release-versions.mjs`. A
  `release: @parity/truapi <version>` therefore also publishes prebuilt
  `truapi-host` binaries through `.github/workflows/release-cli.yml`: one
  archive per target (`aarch64-apple-darwin`, `x86_64-unknown-linux-musl`,
  `aarch64-unknown-linux-musl`) plus a `.sha256`. Each archive includes the
  native binary, self-contained Bun runner and development container.
  Archives are uploaded to the
  `@parity/truapi@<version>` release, followed by the `truapi-host-cli-stable`
  pointer that `scripts/truapi-host-installer.sh` and the CLI's own updater
  read. Release asset URLs must percent-encode the tag
  (`%40parity%2Ftruapi%40<version>`). `make cli-dist CLI_TARGET=<triple>`
  reproduces one archive locally, and `make e2e-cli-update` installs and
  self-updates it against a loopback release server.
- The browser core does not link `verifiable`, whose ring prover compiles in
  4.5 MiB of powers of tau. `truapi-verifiable` holds the four ring-VRF
  operations truapi uses (member, sign, alias, prove). Native builds link
  it; the browser core loads it as a separate WASM module,
  `truapi_verifiable.js` and `truapi_verifiable_bg.wasm` beside the core's own
  files in each bundle (`dist/wasm/web/` or `dist/wasm/testing/`), in the
  background once a pairing session connects or when one of them first runs,
  through `truapi/src/runtime/vrf.rs`. Its loader names both files as
  literal `new URL(…, import.meta.url)` specifiers, so bundlers such as Vite
  emit them. `make wasm` builds the module first and compiles its SHA-256 into
  both cores, which load no other.
- The runtime's WASM artifacts live under
  `js/packages/truapi-host/dist/wasm/web/` and are gitignored.
  Build them locally with `make wasm` (rerun whenever
  `rust/crates/truapi/` changes). CI compiles the crate for
  `wasm32-unknown-unknown` to guard the wasm bridge and its offline subxt
  surface, and the `@parity/truapi-host (wasm bridge)` job builds both bundles
  and runs the package's bun tests against them with `REQUIRE_WASM=1`, so a
  suite needing a bundle fails instead of skipping. `release.yml` rebuilds the
  bundles for a `@parity/truapi-host` release and publishes them in the package.
- The UniFFI bindings and the container bundle are gitignored build outputs.
  After changing UniFFI-exposed types or native bindings, run
  `./ios/truapi-host/scripts/rebuild.sh` to refresh them locally; when only the
  bindings changed, `make uniffi && ./ios/truapi-host/scripts/sync-bindings.sh`
  does that part without Xcode. Because nothing is committed, CI regenerates
  rather than diffs: the `ios-bindings` job proves every UniFFI-exposed type
  still has a binding representation, and the `ios-swift` job generates the
  package's Swift sources and container resource and then compiles the package
  and its test target, which is what catches a hand-written conformer that
  missed a new protocol requirement. On the Kotlin side the `android-bindings`
  job compiles `TrUAPIHost.kt` against freshly generated bindings, which catches
  the same class of drift; `make android-check` does it locally. The embedding
  apps are compiled by neither.
- Both compile gates are path-filtered from one place. The `changes` job in
  `ci.yml` computes `sdk_swift`, `sdk_kotlin`, `needs_changeset`, and
  `adds_changeset`, and each gated job reads the output. Because neither
  binding set is committed, a filter has to name every
  crate its bindings are generated from, since a protocol change leaves no
  `ios/` or `android/` diff to key on. Every job in `ci.yml` is aggregated by
  `ci-status`, which is the check worth requiring: a job skipped by its filter
  counts as a pass, so a gate cannot stall a PR it does not apply to.
  `Changeset guard` reads live PR titles and labels, so re-running failed jobs
  picks up a `no-changeset` opt-out. `Release guard` rejects npm version changes
  with unconsumed changesets on the merge result, including in the merge queue.
  `registry-drift.yml` checks default-branch manifests against npm daily and
  maintains one issue; explicit package-version exceptions live in
  `.github/registry-drift-exceptions.json`. Rust jobs cache through
  `.github/actions/rust-cache`, which saves only on main so every ref restores
  main's entries; release and publish jobs restore without saving. A composite
  action that needs a Rust cache calls Swatinem/rust-cache directly with the
  same main-only `save-if`, because a post step two composites deep loses its
  inputs and never saves. `prune-caches.yml` runs after each CI and iOS CI
  push to main and deletes Rust entries on main that a newer entry of the same
  key family supersedes, so a `Cargo.lock` or toolchain change does not leave
  the old set holding cache space until eviction. See `docs/RELEASE_PROCESS.md` for
  label setup and release recovery.
  Hosts implement `HostBridge`, whose protocol extension defaults the optional
  callbacks; `TrUAPIHostRuntime` and each product execution retain one.
  To publish, include `@parity/ios-host <version>` in the `release:` PR title.
  `release-ios.yml` rebuilds and simulator-tests the XCFramework, uploads it,
  then cuts the plain semver tag `<version>` whose commit carries the generated
  sources and a manifest pointing at that asset. That tag is the SwiftPM
  contract: consumers pin `exact("<version>")`, and a branch cannot be consumed
  directly because the generated sources are ignored there. The job clones and
  compiles the tag before pushing it, then opens a pull request against the
  release branch that points `Package.swift` at the new asset. Dispatching
  `release-ios` manually with a pre-release version cuts a tag for app-side
  testing of an unmerged change without touching any branch. When the title
  also names an npm package, the iOS job waits on that publish being confirmed
  on npm. `publish.sh <version>` is the manual fallback.
  `Package.swift` reads `TRUAPI_USE_LOCAL_BINARY` from the environment to build
  against the rebuilt XCFramework; the tag script refuses a manifest that pins
  the local binary.

## Shared branches

- Treat every remote branch as shared unless told otherwise. Another
  contributor's commits and a reviewer's inline comments both live on it.
- Do not force push. To pick up work that landed on the base branch, merge it
  and resolve the conflicts.
- Before pushing to a branch that already exists, `git fetch origin` and look at
  the remote tip. A push whose result surprises you is a push that lost
  something.
- If a rewrite is ever approved, `--force-with-lease` alone does not protect
  the branch. It compares against the remote tracking ref, which the fetch
  above has just updated, so the lease passes for commits you never saw. Pass
  `--force-if-includes` as well, which additionally requires those commits to
  be part of what you are pushing.
- Create a backup branch before an intentional rewrite. A rewrite drops review
  context even when it preserves every commit.
- Leave unrelated local changes alone. Do not stash, reset or check out over
  work you did not create.

Two ways to lose work that involve no pushing at all:

- `git checkout -- <path>` discards uncommitted changes to that file with no
  prompt and no reflog entry to recover from.
- `git checkout <ref> -- <path>` also stages what it writes, so a later
  `git checkout -- <path>` restores from the index and hands back the version
  from `<ref>` rather than the one you expected. Undo it with
  `git restore --source=HEAD --staged --worktree <path>`.

## Writing for people

- Say what a thing actually is. No invented shorthands, no jargon where a plain
  word exists.
- No em dashes in prose. Use a comma or a full stop.
- Plain engineering prose: no ornate phrasing, stock transitions, inflated
  claims, or private provenance. Say directly what changed, why it matters,
  and what the reader needs to do.
- Link to the document that owns a procedure instead of copying it.

## Pull request titles

- Title every pull request as a conventional commit, with `!` for a breaking
  change. Load `.claude/skills/semver-pr-title/SKILL.md` before opening or
  retitling one; it defines what breaking means here. CI blocks a title that
  does not parse, and a `major` changeset whose title lacks `!`. The nightly
  announcements read `!` to list breaking changes first.

## Scope of a change

- Do not improve adjacent code, comments, or formatting unless asked. Do not
  refactor what is not broken.
- Match the conventions already in the file, even where you would have chosen
  differently.

## Comments

Doc comments on `pub` items are required, per [Code style](#code-style). This is about the
rest.

- An inline comment is the exception, not the default. It earns its place only
  where the code cannot be made to speak for itself, and then it is brief and
  says why, never what.
- Code that is never committed is exempt: a throwaway probe, a scratch script,
  a mutation run. Anything that lands is read by people.
- Documentation explains contracts and non-obvious invariants, not a history
  of how the change was developed.

## Code is read by people and only incidentally run

- Write so a reader needs no comment to follow it. No single-character names,
  no code golf.
- Review what you just wrote and simplify it. Fewer lines is better. If a fix
  feels hacky, redo it as though you had known at the start what you know now.
  Skip this for small obvious changes; do not over-engineer.

## Types and implementation

- Do not alias primitive types. `type Counter = u64` adds another name to
  remember without preventing a counter from being confused with any other
  `u64`. Use the primitive directly, or a newtype when distinguishing values
  or enforcing construction rules provides actual type safety.
- Reuse canonical types, constants, and helpers. Do not introduce a second
  representation or a new dependency just to convert to and from the type the
  surrounding code already uses.
- Keep one source of truth. If one field already determines a fact, do not
  carry a second field describing it and add checks to keep them
  synchronized. Fix the representation.
- Add abstractions and derives for concrete needs, not possible future
  callers. Keep fields private where they protect invariants; do not add
  accessors that merely expose every implementation detail. 
- Keep dependencies in the workspace and inherit them from member crates.
- Prefer safe operations. 
- Fix the underlying cause instead of suppressing a diagnostic or adding a
  fallback that hides an error.

## Establish visibility through module hierarchy

Do not use scoped visibility modifiers such as `pub(crate)`, `pub(super)`, or
`pub(in ...)`. Structure module hierarchies properly and expose the intended
interface through focused public re-exports. Per-item modifiers require
remembering the restriction on every type and item; a proper hierarchy
establishes the boundary once. Scoped modifiers also make it easy to work
around a poorly structured module tree instead of fixing it.

Keep implementation modules private. An item can be `pub` within a private
module without exposing it outside the crate. Re-export the items that belong
to the public interface. Do not make an entire module public to expose one
helper or to make a test compile. Prefer an inherent method when an operation
belongs to an existing type rather than adding another free-standing export.

## Tests

- A test should encode why the behaviour matters, not just what the code does.
  Before changing an existing test, work out why it asserts what it asserts.
- A test that cannot fail when the logic changes is not a test. Check that it
  does.
- Prefer one `assert_eq!` over a whole value to several assertions on
  individual fields.
- In the Android host (`hosts/android`), build test doubles with `mockk` where
  possible, rather than hand-written fakes or Mockito. Instrumentation tests
  get it through `mockk-android`.

## Editing existing Rust

Preserve the local style. Do not add semicolons to `return`, `break` or
`continue` where the file omits them, do not add braces to match arms or
`if`/`else` written without them, and do not move operators between the end of
one line and the start of the next. Format with `cargo +$(cat nightly-toolchain) fmt`, and keep
it to the lines you touched.

## Rust style

- Prefer `derive_more::Display` over a handwritten `fmt::Display`
  implementation when the formatting is declarative. Use a manual
  implementation only when deriving cannot express the behavior cleanly.
## Code style

- Every `pub` Rust item (functions, methods, types, traits, modules, constants) carries a doc comment (`///` or `//!`).
  Keep it short and focused on intent or invariants, not on what the signature already says.
- Do not add code comments or doc comments that narrate migrations, compatibility shims, or historical changes. Comments should describe only the current code.
- Everything else about comments, including when an inline comment earns its place, is under [Comments](#comments). Shared-branch
  hygiene, the scope of a change, tests, and editing existing Rust have their own sections above.
- Remove legacy compatibility code by default. Keep or add it only when explicitly requested.
- In Rust format strings, prefer inlined variables: `"log value: {value:?}"` over `"log value: {:?}", value`.
- For Rust modules, prefer `foo.rs` plus an optional `foo/` directory for
  child modules. Do not introduce new `foo/mod.rs` files unless preserving
  generated output or an existing external convention.
- In runtime Rust code, prefer `core::` over `std::` for types that are
  available in `core` (`core::pin::Pin`, `core::task::Poll`, `core::fmt`, and
  similar). Keep `std::` for std-only APIs, tests, and std-only programs such
  as `truapi-codegen`.
- **No `any` in TypeScript types**: If a type can't be expressed cleanly, stop and ask the user whether to (a) refactor or import the right type or (b) add a scoped `// eslint-disable-next-line @typescript-eslint/no-explicit-any` exception. Never silently leave `any`.
- Don't introduce typealias chains that just rename a public type from another crate (e.g. `pub type StorageError = crate::v01::HostLocalStorageReadError`). Use the canonical name directly. A typealias is only worth its indirection when it captures a real abstraction.
- After any code change, update `README.md` (and this file if the layout changed) so the top-level docs reflect what the repo actually contains. Stale docs are a regression. When moving or removing docs, `rg` for the old path and update or remove stale links in README files, agent notes, skills, comments, and design docs.
- In codegen emitters, prefer `indoc::writedoc!` / `formatdoc!` over chains of `writeln!`. A single `writedoc!` with a multi-line raw string keeps the emitted shape visible in source instead of fragmenting it across one-line `writeln!` calls. Reserve `writeln!` for the genuinely-one-line case (a single import, a single statement inside a loop).
- In PR descriptions, issue comments, and other artifacts that outlive the conversation: describe the resulting state, not the transition between commits. Avoid "previously X, now Y", "we removed", "the old shim is gone", "this PR replaces", those read as ephemeral history once the PR is squash-merged. Write what the system _does_ after the change, not what each commit _changed_ on the way there. (Commit messages are the place for transition narrative; they survive in `git log` even after the squash.)

## Explanation style

- For architecture, event-flow, and debugging explanations, start with a short
  direct summary of the model before diving into long details. Prefer simple
  statements like "the host sends a dirty signal; the core re-reads and derives
  auth state" before listing each hop.
- Use diagrams only when they clarify ownership or message flow. Keep them
  layered and label what is per-tab, shared, host-owned, and core-owned.

## First-time setup

```bash
# Check out the dotli submodule
git submodule update --init --recursive

# Build the TypeScript client (triggers tsc via `prepare`)
( cd js/packages/truapi && npm install )

# Install playground dependencies (picks up @parity/truapi via the file: link)
( cd playground && yarn install --frozen-lockfile )
```

## Regenerating the TS client

When the Rust trait surface changes, rerun:

```bash
./scripts/codegen.sh
```

That will repopulate the ignored generated TS under `js/packages/truapi/src/generated/`,
`js/packages/truapi/src/playground/codegen/`, and `playground/test/generated/examples/`.
After regenerating, rebuild the client and refresh the playground's link copy:

```bash
( cd js/packages/truapi && npm run build )
( cd playground && rm -rf node_modules/@parity && yarn install )
```

(yarn 1.x copies `file:` deps at install time, so the playground's `node_modules/@parity/truapi` is a snapshot.)

## Local development

### Rust

rustfmt and rustdoc JSON run on the dated nightly named in `nightly-toolchain`;
install it once with `rustup toolchain install "$(cat nightly-toolchain)" --component rustfmt,clippy`.
Move the date in that one file, and in `truapi.rustNightly` in
`hosts/android/gradle.properties`, which the Android build checks against it.

```bash
cargo build --workspace
cargo +$(cat nightly-toolchain) fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

### TypeScript client

```bash
cd js/packages/truapi
npm run build
npm test                # bun test suite (src/**/*.test.ts)
```

### Explorer

The explorer is a standalone Vite/React site (no host needed). To run it
locally, just start its own dev server and open the URL directly in a browser.
**Do not** launch dotli for the explorer.

```bash
cd explorer
npx vite --base / --port 5181   # standalone site at http://localhost:5181/
npm run build                    # static export to dist/
```

Use a port other than 5173 (dotli's conventional port) to avoid stale-tab
confusion.

### Playground

```bash
cd playground
yarn dev                # Next.js dev server on :3000
yarn build              # static export to out/
yarn lint
```

The fastest way to exercise the playground is `truapi-host dev -- yarn dev`
from `playground/`. Open `http://localhost:3000` in any existing browser. The
layout's first blocking script loads the CLI bridge and shared container from
`http://127.0.0.1:9955/bootstrap.js`. Dev keeps the app server's assets and hot
reload and automatically approves confirmations. A source build needs the
container bundle generated by `make headless` or `make cli-runner`. On Unix, the
CLI owns the wrapped command's process group and cleans it up with a five-second
SIGTERM grace period.

The playground must otherwise be opened from inside a TrUAPI host. The fastest
local setup is to run dotli's preview server alongside the playground and open
`http://localhost:5173/localhost:3000` in any browser. Use the
[`playground-local-stack`](.claude/skills/playground-local-stack/SKILL.md)
skill to bring both servers up in tmux (it handles the `hosts/dotli/`
submodule init + `bun install` and the per-pane `cd` discipline).
Alternatively, with a deployed Polkadot Desktop Host installed, navigate to
`https://dot.li/localhost:3000` from within it.

#### Local dotli + playground E2E notes

Use `make dev DEBUG=1` from the repo root for the local host stack. It prepares
the ignored WASM/build artifacts, verifies dotli can resolve
`@parity/truapi-host`, then starts dotli on `:5173` and the playground on
`:3000`. Open `http://localhost:5173/localhost:3000`.

When automating with Playwright, block service workers for smoke tests unless
the test is explicitly about SW behavior. Stale host/product bundles can mask
runtime fixes. Use a fresh cache-busting query string on
`http://localhost:5173/localhost:3000?...`, collect `pageerror` and
`console` messages, and fail on unexpected page errors.

For interactive SSO checks, prefer a persistent headed Chrome profile and reuse
the same browser context across checks. SSO pairing needs a real phone QR scan,
and signing/resource-allocation flows may need web or mobile confirmation; if
the human or companion app is unavailable, skip those methods and record the
skip instead of treating it as a protocol failure. Non-interactive checks should
still verify that the playground renders, the TrUAPI debug panel receives
host/product events, generated examples can call non-confirmation methods, and
logout/relogin does not restore a stale session.

The root `make e2e-dotli` target builds the local `truapi-host` binary and
drives the dotli/playground diagnosis through a non-interactive signing-host
CLI process. The CLI answers the QR-derived pairing deeplink, auto-approves
remote requests, stays alive for the SSO session, and is launched again to
verify same-account reconnect after host sign-out. It uses
an explicitly exported `HOST_CLI_SIGNER_MNEMONIC` when present. Without one,
it auto-manages a reusable isolated identity under `.e2e-dotli/`. Set
`E2E_DOTLI_SIGNING_HOST_BASE_PATH` to preserve and reuse signing-host state
while debugging. Use `E2E_DOTLI_SMOKE=1 make e2e-dotli` for the QR-only smoke
path.

For a fully automated local playground diagnosis run, use:

```bash
make e2e-dotli
```

`make e2e-dotli` starts dotli preview and the playground, signs out any
restored host session, signs in through the local signing-host CLI by extracting
the QR payload, runs the playground Diagnosis screen, auto-accepts host-side
Allow/Sign modals, and writes
`playground/test-results/e2e-dotli/diagnosis-report.md`.

Any CI job running the same target needs `DOTLI_CHECKOUT_TOKEN` for private
submodule checkout; without dotli access it should skip this integration gate
rather than fail unrelated checks.

A useful no-phone smoke assertion is:

```bash
E2E_DOTLI_SMOKE=1 make e2e-dotli
```

For manual debugging of that smoke path:

1. Start `make dev DEBUG=1`.
2. Open `http://localhost:5173/localhost:3000?debug=truapi&cachebust=<ts>` with
   service workers blocked.
3. Wait for `globalThis.__truapi?.setLogLevel`, call
   `__truapi.setLogLevel("debug")`, and confirm the console logs
   `[truapi worker] logLevel=debug providers=0`.
4. Click `#auth-button`, wait for `#auth-modal-backdrop.open`, and confirm:
   the modal shows `Login with Polkadot Mobile`, `__truapi.getProviderCount()`
   is greater than zero, worker frame/callback logs appear, and there are no
   page errors.

If `make dev` reports `EADDRINUSE` on `:5173` or the playground moves from
`:3000` to `:3001`, kill stale `preview-server.ts` / `next dev` processes and
restart the tmux session. Port drift causes false-negative local e2e results.

Useful debug signals:

```js
__truapi.setLogLevel("debug");
sessionStorage.setItem("dotli:truapi-debug", "1");
```

Reload after setting the debug-panel flag. Watch for `unknown wire discriminant pair`, missing
`@parity/truapi-host` imports, worker WASM instantiation failures, and
debug-panel traffic disappearing when the login popup opens.

## Deployment

Pushes to `main` trigger `.github/workflows/deploy-playground.yml`, which builds `playground/` and publishes the static export via `bulletin-deploy`. Pass the bare dotNS label `truapi-playground`, never a suffixed name: dotNS attaches the top-level domain its network declares, so the live name is `truapi-playground.paseo` on Paseo Next v2. The deploy steps stay in this repo because `bulletin-deploy` ships its shared reusable workflow from a private repo that this public one cannot call.
Pushes to `main` also trigger `.github/workflows/deploy-docs.yml`, which publishes the explorer (at the Pages root), the playground (under `/playground/`), and the Rust API docs (under `/cargo_doc/`) to GitHub Pages.
