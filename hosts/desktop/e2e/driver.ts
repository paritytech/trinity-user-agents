/**
 * REPL driver for the app — one long-lived Electron instance, driven by lines on stdin.
 * Manual: .claude/skills/run-app/SKILL.md
 *
 * With `RENDERER_SOURCE=localhost` + a running `npm run start:web`, the renderer
 * hot-reloads inside the real Electron shell (main process, preload and IPC all live).
 *
 * Usage:
 *   npx tsx e2e/driver.ts                      # interactive
 *   npx tsx e2e/driver.ts < some.fifo > out.log  # agent-driven
 */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { pathToFileURL } from 'node:url';

import { type ElectronApplication, type Page, _electron as electron } from 'playwright-core';

const APP_DIR = process.env['APP_DIR'] ?? process.cwd();
const SHOT_DIR = process.env['SCREENSHOT_DIR'] ?? path.join(os.tmpdir(), 'polkadot-desktop-shots');
fs.mkdirSync(SHOT_DIR, { recursive: true });

let app: ElectronApplication | null = null;
let page: Page | null = null;
const logs: string[] = [];
let runCount = 0;

const env = Object.fromEntries(
  Object.entries({ ...process.env, NODE_ENV: 'development' }).filter(
    (entry): entry is [string, string] => entry[1] !== undefined,
  ),
);

const COMMANDS: Record<string, (arg: string) => Promise<void> | void> = {
  async launch() {
    if (app) {
      console.info('already launched');

      return;
    }
    app = await electron.launch({
      args: [path.join(APP_DIR, 'release/build/main.cjs')],
      env,
      timeout: 60_000,
    });
    page = await app.firstWindow({ timeout: 60_000 });
    page.on('console', m => logs.push(`[${m.type()}] ${m.text()}`));
    page.on('pageerror', e => logs.push(`[pageerror] ${e.message}`));
    page.on('crash', () => logs.push('[crash] renderer crashed'));
    await page.waitForLoadState('domcontentloaded');
    // domcontentloaded fires before React paints — screenshotting there gives a blank frame.
    await page.waitForFunction(() => (document.body.textContent ?? '').trim().length > 0, { timeout: 30_000 });
    console.info('launched:', page.url());
  },

  async ss(name) {
    const file = path.join(SHOT_DIR, (name || `ss-${logs.length}`) + '.png');
    await page!.screenshot({ path: file });
    console.info('screenshot:', file);
  },

  async click(selector) {
    await page!.locator(selector).first().click({ timeout: 10_000 });
    console.info('clicked', selector);
  },

  async 'click-text'(text) {
    await page!.getByText(text, { exact: false }).first().click({ timeout: 10_000 });
    console.info('clicked text', text);
  },

  async type(text) {
    await page!.keyboard.type(text, { delay: 30 });
  },

  async press(key) {
    await page!.keyboard.press(key);
  },

  async wait(selector) {
    await page!.waitForSelector(selector, { timeout: 15_000 });
    console.info('found:', selector);
  },

  async eval(expression) {
    console.info(JSON.stringify(await page!.evaluate(expression)));
  },

  async text(selector) {
    console.info(await page!.evaluate(s => (s ? document.querySelector(s) : document.body)?.textContent, selector || null));
  },

  url() {
    console.info(page!.url());
  },

  async reload() {
    await page!.reload();
    console.info('reloaded:', page!.url());
  },

  console(count) {
    console.info(logs.slice(-(Number(count) || 40)).join('\n'));
  },

  async windows() {
    for (const w of app!.windows()) console.info(' win:', w.url());
    const contents = await app!.evaluate(({ webContents }) =>
      webContents.getAllWebContents().map(w => ({ id: w.id, type: w.getType(), url: w.getURL() })),
    );
    for (const w of contents) console.info(` wc[${w.id}] ${w.type}: ${w.url}`);
  },

  // Run a whole flow in one round trip: a .ts file default-exporting (page, app) => Promise.
  // Playwright auto-waits, so a scripted flow needs no sleeps between steps.
  async run(file) {
    const url = pathToFileURL(path.resolve(APP_DIR, file)).href;
    // Unique query per call — otherwise an edited script re-runs from the ESM module cache.
    const module = await import(`${url}?v=${(runCount += 1)}`);
    await module.default(page, app);
    console.info('ran:', file);
  },

  async quit() {
    await app?.close().catch(() => {});
    app = null;
    page = null;
  },

  help() {
    console.info('commands:', Object.keys(COMMANDS).join(', '));
  },
};

// Electron steals process.stdin — read the raw fd instead.
const stdin = fs.createReadStream('', { fd: fs.openSync('/dev/stdin', 'r') });
const rl = readline.createInterface({ input: stdin, output: process.stdout, prompt: 'driver> ' });

async function handle(line: string) {
  const [cmd, ...rest] = line.trim().split(/\s+/);
  if (!cmd) return;

  const startedAt = Date.now();
  const command = COMMANDS[cmd];
  if (!command) {
    console.info('unknown:', cmd);
  } else if (!app && !['launch', 'help', 'quit'].includes(cmd)) {
    console.info('ERROR: launch first');
  } else {
    try {
      await command(rest.join(' '));
    } catch (e) {
      console.info('ERROR:', e instanceof Error ? e.message.split('\n')[0] : e);
    }
  }
  // Completion marker: callers wait for N of these instead of guessing with sleep.
  console.info(`<<done ${Date.now() - startedAt}ms>>`);
  if (cmd === 'quit') process.exit(0);
}

// readline does not await the handler, so a batch of lines would otherwise run concurrently.
let queue = Promise.resolve();
rl.on('line', line => {
  queue = queue.then(() => handle(line)).then(() => rl.prompt());
});

// Keep the REPL alive when the app dies — a crash is a finding to inspect, not a reason to exit.
process.on('uncaughtException', e => console.info('FATAL:', e.message));
process.on('unhandledRejection', e => console.info('FATAL:', e));

console.info('driver ready — "help" for commands');
rl.prompt();
