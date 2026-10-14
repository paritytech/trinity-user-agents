# TrUAPI Playground

_Browse, edit, and call App-compatible TrUAPI methods live against a connected Polkadot host._

The playground is an interactive reference for the App-compatible TrUAPI surface: methods are grouped by domain, with live request payload editing, one-click calls, and live subscriptions. Production builds run inside a TrUAPI host. Local development can also use the CLI browser bridge in a plain browser tab.

**Live app:** [https://truapi-playground.paseo.li/](https://truapi-playground.paseo.li/)

## Features

- **Execution-aware method browser**: every TrUAPI service available to an `App` execution, each with a description and a Request / Response or Subscription badge.
- **Live calls**: edit a JSON request payload and fire the call against the connected host.
- **Subscriptions**: open and close streaming methods and watch events arrive in real time.
- **Auto-test view**: runs every listed method and reports pass / fail in one pass.
- **Diagnosis view**: runs the App surface and produces a copy-pasteable markdown report per host. The explorer's Compatibility page aggregates those into a cross-host matrix. See [Diagnosis](#diagnosis).
- **Wiring status**: methods that are not yet bound are flagged "Not supported" so you can see protocol coverage at a glance.
- **Chat diagnosis**: the same build emits `out/worker/index.js`, a `Worker`
  executable that tests room creation and idempotency, bot registration and
  idempotency, live room-list updates, text and renderer-drawn messages, user
  actions, and host-initiated `Renderer` streams. It displays live results in
  Chat and posts a Chat-only Markdown report after `!diagnose` completes the
  action check.

## Local development

```bash
yarn install --frozen-lockfile
truapi-host dev -- yarn dev
```

Open [http://localhost:3000](http://localhost:3000). `truapi-host dev` starts a
signing host on `127.0.0.1:9955`, waits until its signer is ready, then starts
the playground. The development-only tag in `src/app/layout.tsx` loads
`http://127.0.0.1:9955/bootstrap.js`, which installs the same
`window.__HOST_API_PORT__` used by native webview hosts. The production build
checks that this bridge URL is absent from `out/`.

The source tag and the CLI use fixed port `9955`. If you pass a different
`truapi-host dev --port`, update the tag to match. The CLI accepts frame
connections only from local TCP peers. Browser WebSocket origins must also be
`localhost` or a loopback IP. Local non-browser clients may omit `Origin`.

To exercise a real host instead, run `yarn dev` and open the dev server inside
the Polkadot Desktop Host:

```
https://dot.li/localhost:3000
```

Opening the page directly without either host does not work.

The plain-browser Playwright suite starts the installed CLI and owns the whole
stack:

```bash
yarn e2e:cli
TRUAPI_HOST_BIN=../target/debug/truapi-host yarn e2e:cli
```

`TRUAPI_HOST_BIN` selects a checkout build when needed. Existing listeners are
rejected so the suite cannot silently test a partial or unrelated stack.
`yarn e2e:cli-diagnosis` runs the longer compatibility diagnosis. It records
failed and skipped methods as findings and exits nonzero only when the run
itself breaks or the page raises an error. The CLI Playwright suite is the
regression gate.

`yarn build` produces both the static app under `out/` and its `Worker` executable
at `out/worker/index.js`. Both resolve `@parity/truapi` from the linked
workspace package in `../js/packages/truapi`.

The browser and Chat diagnoses are intentionally separate. Generated service
metadata carries `requiredExecution`; the browser omits services requiring
`Chat`, while the worker tests only the Chat service in its trusted Chat
connection.

## Adding a method

Methods reach the playground via codegen — there is no per-method wiring file to edit. The flow:

1. Edit the trait in [`rust/crates/truapi/src/api/<service>.rs`](../rust/crates/truapi/src/api/) and include a ` ```ts ` rustdoc block on the method. That block becomes the playground's runnable example (the editor contents you see in the **Example** tab).
2. From the repo root, run [`../scripts/codegen.sh`](../scripts/codegen.sh). This regenerates the TS client, the playground metadata re-exported from [`src/lib/services.ts`](src/lib/services.ts), and the per-method example files under `test/generated/examples/`.
3. Rebuild the client and refresh the playground's `file:` snapshot of it:

   ```bash
   ( cd ../js/packages/truapi && npm run build )
   ( cd . && rm -rf node_modules/@parity && yarn install )
   ```

A method without a `ts` rustdoc block shows up with a "Not supported" badge — there is no example to run until you add one.

### Example conventions

An example **passes** when its promise resolves and **fails** when it throws. Use the ambient `assert(condition, ...message)` (no import) to fail explicitly — `assert(false, ...)` throws. `console.*` is pure output. For a `Result`, write `assert(r.isOk(), "<step> failed:", r)` (narrows `r` to `Ok`, includes the result in the failure message). Await subscriptions with `firstValueFrom(from(<observable>))`.

## Diagnosis

The Diagnosis view exercises every App-compatible TrUAPI method against the connected host and emits a per-host pass/fail report you can copy out. Per-host reports feed the explorer's **Compatibility** page, which renders the host × method matrix; aggregation lives in the explorer (see [`explorer/README.md`](../explorer/README.md#host-compatibility-matrix)). Chat APIs are diagnosed separately by the `Worker` executable.

Run the iOS Chat diagnosis from the repository root:

```bash
make ios-chat-run
```

Select a prepared simulator by name or UDID when more than one runtime is
installed. The validated local target is iOS 18.3; iOS 26 can surface an
unrelated keychain/onboarding reset before Chat starts.

```bash
make ios-chat-run IOS_SIMULATOR_DEVICE="TrUAPI SSO E2E 18.3"
```

When no device is specified, the launcher prefers an available simulator whose
name contains both `TrUAPI` and `E2E`, then falls back to a booted iPhone.

It writes a Chat-only report to
`playground/test-results/ios-chat/diagnosis-report.md`. The native diagnosis
widget also provides **Copy report**; save the result as
`explorer/diagnosis-reports/chat/ios.md` to update the explorer's separate Chat
compatibility section.

Open the playground inside a TrUAPI host (it cannot run standalone in a browser tab):

- **Web host:** [https://truapi-playground.paseo.li/](https://truapi-playground.paseo.li/) opened inside dot.li.
- **Desktop host:** the Polkadot Desktop app pointed at the playground URL.

Before you start:

- Make sure you are **logged in** to the host.
- Keep your **phone nearby** — the disruptive methods (signing, permission requests) will prompt the Polkadot mobile app and the diagnosis will wait for you to approve each one.

Then, in the playground:

1. Click **Diagnosis** in the left sidebar (below Auto-Test, above the service list).
2. Read the instructions on the screen, then click **Run diagnosis**.
3. Wait for the run to finish. Non-disruptive methods run in parallel first, then disruptive methods run one at a time — approve each pop-up on your phone as it appears. A live log updates per method (`queued → processing… → success / failed`).
4. When the run finishes, a **Report** panel appears above the log. Click **Copy report**.
5. Click **Submit report ↗** to file a pre-filled GitHub issue that the `diagnosis-report` workflow turns into a per-host PR under `explorer/diagnosis-reports/`. (Or click **Copy report**, save the markdown to a host-named file like `spa/web.md`, and update the matrix by hand — see [`../explorer/README.md`](../explorer/README.md#updating-the-matrix).)

The report looks like this:

```markdown
## Truapi Web Diagnosis

| Method                      | Status |
| --------------------------- | ------ |
| `Account/get_account`       | ✅     |
| `Account/get_account_alias` | ❌     |
| `System/handshake`          | ✅     |

...
```

| Icon | Meaning                                                                                                   |
| ---- | --------------------------------------------------------------------------------------------------------- |
| ✅   | The method ran and returned a successful result.                                                          |
| ❌   | The method failed — it errored at runtime, the host returned an error, or it has no runnable example yet. |

The host mode in the title (`Web` / `Desktop`) is detected automatically — Electron in the user-agent or the native-webview marker ⇒ Desktop, browser iframe ⇒ Web.

## Deploy

Pushes to `main` deploy automatically via the [Deploy Playground workflow](../.github/workflows/deploy-playground.yml). The steps below mirror that workflow and let you ship out-of-band, for example to test a branch against the live DotNS name.

### Prerequisites

- Node.js 22 (matches CI).
- `bulletin-deploy` installed globally: `npm install -g bulletin-deploy`.

### Deploy from local

```bash
yarn install --frozen-lockfile
yarn build
bulletin-deploy ./out truapi-playground.paseo --js-merkle
```

The build output goes to `./out`. The deploy can fail on transient network errors; CI retries up to 3 times, and you can simply rerun the command locally.

The name carries its TLD because [`bulletin-deploy.config.ts`](bulletin-deploy.config.ts) is present: the deploy then also publishes the product manifest, and the publisher requires the name passed here to match the config's `domain` exactly. `.paseo` is the TLD of the default environment (`paseo-next-v2`); for another environment, pass the matching name and set `PLAYGROUND_DOTNS_NAME` to the same value so the config follows, as [the workflow](../.github/workflows/deploy-playground.yml) does.

Publishing the manifest registers the `app.` and `worker.` subnames, so the signer has to own the base name and cover the registration deposits. Pass `--content-only` to update the site alone and leave the manifest untouched.

### Quick iteration

`deploy:test` skips `--js-merkle`, stays content-only, and cleans up the generated `out.car`:

```bash
yarn deploy:test
```

## License

[MIT](../LICENSE)
