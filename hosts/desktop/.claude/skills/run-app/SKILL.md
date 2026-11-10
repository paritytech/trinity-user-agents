---
name: run-app
description: Use when asked to run/launch the Electron app, take a screenshot of it, click through a flow, or visually validate a change in the real app (not just tests). Drives the app through the Playwright REPL at e2e/driver.ts.
---

# Running and validating the app

The app is Electron. Launching it is not enough — drive it and **look at a screenshot**,
otherwise you have only proven the entrypoint resolves.

`e2e/driver.ts` is a REPL: one long-lived Electron instance, one command per line on stdin.
The app takes ~10s to boot, so a REPL beats relaunching per interaction.

With `RENDERER_SOURCE=localhost` the main process loads the vite dev server
(`main/factories/window.ts`), so **renderer edits hot-reload inside the real Electron shell**
— main process, preload and IPC all live. Only `main/` edits need a rebuild + relaunch.

## Prerequisites

```bash
npm run r main:dev preload:dev callwin-preload:dev   # once, and after any main/ or preload change
nohup npm run start:web > /tmp/vite-dev.log 2>&1 &   # https://localhost:4000, stays up across turns
```

Both are required. A stale `release/build/main.cjs` crashes the renderer mid-flow; a dead dev
server lands the window on `chrome-error://chromewebdata/`.

## Run it (agent path)

The driver reads stdin; a FIFO lets you feed it commands across turns. Keep a holder process on
the write end so the driver never sees EOF:

Start the driver **and** drive it in the same call — the app is interactive 0.2s after it renders,
so any wait you see between the two is your own turn latency, not the app:

```bash
FIFO=$SCRATCHPAD/in.fifo; LOG=$SCRATCHPAD/out.log
mkfifo $FIFO
nohup sh -c "sleep 7200 > $FIFO" >/dev/null 2>&1 &
RENDERER_SOURCE=localhost SCREENSHOT_DIR=$SCRATCHPAD/shots \
  nohup npx tsx e2e/driver.ts < $FIFO > $LOG 2>&1 &

until grep -aq "driver ready" $LOG; do sleep 0.05; done      # not `sleep 3`
printf "launch\nrun $SCRATCHPAD/flow.ts\n" > $FIFO            # launch AND the flow, one call
until [ "$(grep -ac '<<done' $LOG)" -ge 2 ]; do sleep 0.05; done
```

Cold shell → app driven through a 6-step flow: **4.6s**. Splitting that across two calls costs a
full turn for nothing.

**Never `sleep` to wait for a command.** The driver prints `<<done Nms>>` after each one. Count the
baseline, send, wait for the count to rise:

```bash
wait_done () { until [ "$(grep -ac '<<done' $LOG)" -ge "$1" ]; do sleep 0.05; done; }

B=$(grep -ac '<<done' $LOG)
printf 'click-text Skip (Dev only)\nss dashboard\n' > $FIFO   # one command per line, run in order
wait_done $((B+2)); tail -5 $LOG
```

Then `Read` the PNG path the log prints. Screenshots default to
`$TMPDIR/polkadot-desktop-shots/` (`SCREENSHOT_DIR` overrides).

Human path: `RENDERER_SOURCE=localhost npx tsx e2e/driver.ts` and type commands.

## Going fast

Measured boot: window at 1.2s, onboarding rendered at 3.1s, **first click landing 0.22s after that**.
Per-command work is 2–140ms. So any longer pause you observe is **turn count**, not the app — spend
turns, not milliseconds. In order of leverage:

1. **`run <file.ts>` for anything over ~2 steps.** Write a flow file, execute it against the
   already-booted app in one turn. Playwright auto-waits, so it needs no sleeps between steps. A
   6-step flow ending in a screenshot: **1.7s, one turn.**

   ```ts
   import { type Page } from 'playwright-core';

   export default async function (page: Page) {
     if (page.url().includes('onboarding')) {
       await page.getByText('Skip (Dev only)').click();
       await page.waitForURL(/dashboard/);
     }
     await page.getByTestId('address-bar-input').click();
     await page.keyboard.type('coin');
     await page.getByText('Open', { exact: false }).first().waitFor();
     await page.screenshot({ path: process.env['SCREENSHOT_DIR'] + '/flow.png' });
   }
   ```

   Edit the file and `run` it again — it reloads, no relaunch.
2. **Batch lines** when you do use plain commands — they execute in order, one round trip.
3. **Never relaunch** to get back to a clean state; `reload` (~1s) or navigate.
4. **A wait that fails costs its whole timeout** (15s for `wait`, 30s inside a script) and dwarfs
   everything else. When a step hangs, the selector is wrong or the app is in an unexpected state —
   `ss` + `console` instead of retrying blind.

## Commands

| command | what it does |
| --- | --- |
| `launch` | launch Electron, wait for first paint, print the URL |
| `ss [name]` | screenshot → `$SCREENSHOT_DIR/<name>.png` |
| `click <sel>` / `click-text <text>` | click by selector / visible text |
| `type <text>` / `press <key>` | keyboard input (`press Meta+K`) |
| `wait <sel>` | wait for a selector, 15s |
| `run <file.ts>` | run a flow file default-exporting `(page, app) => Promise` — the fast path |
| `eval <js>` / `text [sel]` | evaluate in the page / print textContent |
| `url` / `reload` | current URL / reload the renderer |
| `console [n]` | last n console + pageerror + crash lines |
| `windows` | list windows and webContents — finds product webviews |
| `quit` | close the app (driver stays alive) |

## Validating a change

1. Reach the state your change touches (`click-text`, `type`, `press`) and `ss` it — **read the
   PNG**. A blank frame means it didn't render, not that it passed.
2. `console` after the interaction — a silent visual pass with a `[pageerror]` in the log is a fail.
3. For flows the BDD suite already covers, run that instead of hand-driving:
   `npm run test:e2e:smoke` / `:browser` / `:authenticated` (see `e2e/CLAUDE.md`). Sign-in needs a
   `truapi-host` signer, which only the suite's fixtures spawn — the driver has no auth.
4. Clean up when done: `pkill -f e2e/driver.ts; pkill -f release/build/main.cjs; pkill -f renderer:dev`.

## Gotchas

- **Onboarding blocks everything** — a fresh launch starts on `/onboarding`; `click-text Skip (Dev
  only)` lands on `/dashboard`. Make flow files check `page.url()` rather than assume.
- **State bleeds between runs.** The app reuses the real profile (open tabs, typed text, dashboard
  layout all survive), so a flow that assumes an empty field silently acts on the wrong state and
  the next `waitFor` eats its full timeout. Either reset in the flow (`Meta+A` before typing) or
  pass `ELECTRON_USER_DATA=$(mktemp -d)` for a throwaway profile — at the cost of re-onboarding.
- **Test IDs sit on wrappers.** `getByTestId('address-bar-input').fill(...)` fails — the testid is
  on a div. Click it and use `keyboard.type`, or drill down with `.locator('input')`.
- **Do not truncate the log while the driver holds it open** (`: > $LOG`). The file becomes sparse,
  `grep -c` reports it as binary, and the wait loop never terminates. Count from a baseline instead.
- **The dev server dies with its parent shell.** Start it with `nohup … &` and never inside a
  command that a timeout may kill — the whole process group goes with it.
- **Product webviews are separate pages.** `windows` lists them; the driver only drives the main
  window. Attaching to a webview needs a new command.
- **`https://localhost:4000` uses a self-signed cert.** Electron accepts it; `curl` needs `-k`.
