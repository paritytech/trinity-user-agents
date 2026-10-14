#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Drive the Chat Extension v2 sample product on the CLI chat page, headless.
//
// The launcher starts `truapi-host dev` with the worker bundle of
// `playground/chat-v2`, opens the `/chat` page in headless Chromium (the
// page boots the worker, as it does for a developer), types the commands of
// the sample product into the page's composer, and checks what the page
// draws and what the worker logs. It writes one diagnosis row per scenario
// of the test plan that the CLI page covers, with a screenshot each, to
// `playground/test-results/cli-chat/diagnosis-report.md`.
//
// Usage:
//   node scripts/launch-cli-chat-playground.mjs [--host-bin PATH] [--port N]
//   node scripts/launch-cli-chat-playground.mjs --url http://127.0.0.1:9971/chat
//
// `--url` drives a host that already runs; otherwise the launcher starts one
// and stops it at the end. The launcher builds the product when its worker
// bundle is not there, or always with `--build`.

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { createServer } from "node:net";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CLI_CHAT_SCENARIOS,
  renderScenarioReport,
  scenarioFailures,
} from "./lib/cli-chat-scenarios.mjs";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const PRODUCT_DIR = join(ROOT, "playground/chat-v2");
const WORKER_BUNDLE = join(PRODUCT_DIR, "out/worker/index.js");
const OUT_DIR = join(ROOT, "playground/test-results/cli-chat");
const REPORT = join(OUT_DIR, "diagnosis-report.md");
const PRODUCT_ID = "truapi-chat-v2.dot";

function option(name) {
  const index = process.argv.indexOf(name);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const hostBin =
  option("--host-bin") ??
  process.env.TRUAPI_HOST_BIN ??
  join(ROOT, "target/debug/truapi-host");
const givenUrl = option("--url");

function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolvePort(port));
    });
  });
}

function buildProduct() {
  if (existsSync(WORKER_BUNDLE) && !process.argv.includes("--build")) return;
  for (const args of [["install", "--frozen-lockfile"], ["build"]]) {
    const result = spawnSync("yarn", args, {
      cwd: PRODUCT_DIR,
      stdio: "inherit",
    });
    if (result.status !== 0)
      throw new Error(`yarn ${args.join(" ")} failed in playground/chat-v2`);
  }
}

/** Start `truapi-host dev` and resolve once it serves the chat page. */
async function startHost(port) {
  const log = join(OUT_DIR, "host.log");
  const chatLog = join(OUT_DIR, "host-chat.jsonl");
  const child = spawn(
    hostBin,
    [
      "dev",
      "--port",
      String(port),
      "--product-id",
      PRODUCT_ID,
      "--worker-bundle",
      WORKER_BUNDLE,
    ],
    {
      cwd: ROOT,
      env: { ...process.env, TRUAPI_CHAT_LOG: chatLog },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  const lines = [];
  const collect = (chunk) => {
    lines.push(chunk.toString());
    writeFileSync(log, lines.join(""));
  };
  child.stdout.on("data", collect);
  child.stderr.on("data", collect);
  const deadline = Date.now() + 180_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null)
      throw new Error(
        `truapi-host dev exited with ${child.exitCode}; see ${log}`,
      );
    if (lines.join("").includes("Serving product frames")) return child;
    await sleep(250);
  }
  child.kill("SIGTERM");
  throw new Error(`truapi-host dev did not serve within 180 s; see ${log}`);
}

function sleep(ms) {
  return new Promise((done) => setTimeout(done, ms));
}

/**
 * Chromium from the first Playwright install whose browser is downloaded:
 * the one `js/packages/truapi-host` uses, then the playground's.
 */
function loadChromium() {
  const candidates = [
    [join(ROOT, "js/packages/truapi-host/package.json"), "playwright"],
    [join(ROOT, "playground/package.json"), "@playwright/test"],
  ];
  const tried = [];
  for (const [from, name] of candidates) {
    try {
      const { chromium } = createRequire(from)(name);
      if (existsSync(chromium.executablePath())) return chromium;
      tried.push(`${name}: no browser at ${chromium.executablePath()}`);
    } catch (error) {
      tried.push(`${name}: ${error.message.split("\n")[0]}`);
    }
  }
  throw new Error(
    `no Playwright Chromium; run \`npx playwright install chromium\`\n${tried.join("\n")}`,
  );
}

/** One page with its worker, and the helpers the scenarios use. */
class ChatPage {
  constructor(page) {
    this.page = page;
    this.log = [];
    this.console = [];
    page.on("console", (message) => {
      const text = message.text();
      this.console.push(`[${message.type()}] ${text}`);
      if (text.startsWith('{"t":"chat-v2"')) {
        try {
          this.log.push(JSON.parse(text));
        } catch {
          // Not a log line of the product.
        }
      }
    });
    page.on("pageerror", (error) =>
      this.console.push(`[pageerror] ${error.message}`),
    );
  }

  /** Wait for a log line after index `since` that `match` accepts. */
  async logLine(match, since = 0, timeoutMs = 10_000) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      const line = this.log.slice(since).find(match);
      if (line) return line;
      await sleep(100);
    }
    throw new Error(`no worker log line matched within ${timeoutMs} ms`);
  }

  async room(roomId) {
    await this.page.click(`[data-room-id="${roomId}"]`);
    await this.page.waitForSelector(`[data-room-id="${roomId}"].active`);
  }

  async reopen(roomId, other) {
    await this.room(other);
    await this.room(roomId);
  }

  async type(text) {
    await this.page.fill("#input", text);
    await this.page.press("#input", "Enter");
  }

  /** Type a command and wait until the worker logged the event or action it caused. */
  async command(roomId, text) {
    const since = this.log.length;
    await this.type(text);
    return this.logLine(
      (line) =>
        line.room === roomId &&
        ((line.via === "action_subscribe" &&
          (line.kind === "MessagePosted" || line.kind === "Command")) ||
          (line.via === "user_action" && line.kind === "NewMessage")),
      since,
    );
  }

  /** The messages the thread draws, oldest first. */
  messages() {
    return this.page.$$eval("#thread [data-message-id]", (nodes) =>
      nodes.map((node) => ({
        id: node.dataset.messageId,
        origin: node.dataset.origin,
        status: node.dataset.status,
        temp: node.dataset.temp === "true",
        edited: node.dataset.edited === "true",
        body: node.querySelector("[data-body]")?.dataset.body ?? null,
        text: node.querySelector(".bubble")?.innerText ?? "",
      })),
    );
  }

  async until(check, what, timeoutMs = 10_000) {
    const deadline = Date.now() + timeoutMs;
    let last;
    while (Date.now() < deadline) {
      last = await check();
      if (last) return last;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${what}`);
  }

  async shot(name) {
    const path = join(OUT_DIR, name);
    await this.page.screenshot({ path });
    return relative(OUT_DIR, path);
  }
}

/** Run one scenario and give its row. */
async function scenario(rows, definition, run) {
  const started = Date.now();
  try {
    const evidence = await run();
    rows.push({
      ...definition,
      result: "pass",
      durationMs: Date.now() - started,
      evidence,
    });
    console.log(
      `pass ${definition.id} ${definition.title} (${definition.mode})`,
    );
  } catch (error) {
    rows.push({
      ...definition,
      result: "fail",
      durationMs: Date.now() - started,
      evidence: String(error?.message ?? error),
    });
    console.log(
      `fail ${definition.id} ${definition.title}: ${error?.message ?? error}`,
    );
  }
}

const byId = Object.fromEntries(
  CLI_CHAT_SCENARIOS.map((definition) => [definition.id, definition]),
);

async function drive(chat) {
  const rows = [];
  const { page } = chat;

  // S45: the page opens `source` as soon as the worker declares it, so the
  // first query reaches a worker that has only just evaluated its bundle.
  await scenario(rows, byId.S45, async () => {
    await page.waitForSelector('[data-room-id="host"]', { timeout: 30_000 });
    await page.waitForSelector('[data-room-id="source"].active', {
      timeout: 30_000,
    });
    const query = await chat.logLine(
      (line) => line.via === "messages_query" && line.room === "source",
    );
    await chat.logLine(
      (line) =>
        line.via === "answer" && line.re === query.seq && line.kind === "Page",
    );
    await chat.until(
      async () => (await page.$(".paging.err")) === null,
      "no load error",
    );
    return `${await chat.shot("s45-source.png")}; query seq ${query.seq} answered with a page`;
  });

  // A known start: no slow answers, an empty `source`.
  await chat.room("source");
  await chat.command("source", "/slow 0");
  await chat.command("source", "/history 0");
  await chat.reopen("source", "host");
  await chat.until(
    async () => (await page.$("#placeholder")) !== null,
    "the empty source room",
  );

  await scenario(rows, byId.S15, async () => {
    await chat.command("source", "/history 120");
    const since = chat.log.length;
    await chat.reopen("source", "host");
    const first = await chat.logLine(
      (line) =>
        line.via === "messages_query" &&
        line.room === "source" &&
        line.kind === "Before",
      since,
    );
    if (first.data.anchor.value.cursor !== null)
      throw new Error(
        `first page asked from ${first.data.anchor.value.cursor}`,
      );
    const page1 = await chat.until(async () => {
      const messages = await chat.messages();
      return messages.length === 40 ? messages : null;
    }, "a page of 40 messages");
    if (page1.at(-1).id !== "source-h-00120")
      throw new Error(`newest message is ${page1.at(-1).id}`);
    const thread = await page.$eval(
      "#thread",
      (node) => node.scrollHeight - node.scrollTop - node.clientHeight,
    );
    if (thread > 80)
      throw new Error(`the thread is ${thread}px above its bottom`);
    await page.click("#older");
    await chat.until(
      async () => (await chat.messages()).length === 80,
      "a second page",
    );
    const asked = chat.log
      .slice(since)
      .filter((line) => line.via === "messages_query" && line.room === "source")
      .map((line) => line.data.anchor.value.cursor ?? "newest");
    if (new Set(asked).size !== asked.length)
      throw new Error(`a page was asked twice: ${asked.join(", ")}`);
    return `${await chat.shot("s15-source.png")}; pages asked ${asked.join(", ")}`;
  });

  await scenario(rows, byId.S19, async () => {
    await chat.command("source", "/slow 1500");
    const since = chat.log.length;
    await chat.type("Hello from the CLI page");
    const preparing = await chat.until(
      async () => {
        const messages = await chat.messages();
        return messages.find(
          (message) => message.temp && message.status === "Preparing",
        );
      },
      "a Preparing bubble",
      1_400,
    );
    await chat.shot("s19-preparing.png");
    const created = await chat.logLine(
      (line) =>
        line.via === "answer" &&
        line.room === "source" &&
        line.kind === "MessageCreated",
      since,
      10_000,
    );
    const id = created.data.messageId;
    await chat.until(async () => {
      const messages = await chat.messages();
      return (
        messages.some(
          (message) => message.id === id && message.status === "Sent",
        ) && !messages.some((message) => message.id === preparing.id)
      );
    }, `${id} in place of ${preparing.id}`);
    await chat.command("source", "/slow 0");
    return `${await chat.shot("s19-created.png")}; ${preparing.id} became ${id}`;
  });

  // The host-store room.
  await chat.room("host");
  await chat.command("host", "/slow 0");

  await scenario(rows, byId.S7, async () => {
    await chat.command("host", "/post all");
    const kinds = await chat.until(
      async () => {
        const found = await page.evaluate(() => ({
          text: !!document.querySelector(
            '#thread [data-origin="product"] [data-body="Text"]',
          ),
          markdown:
            !!document.querySelector("#thread .md strong") &&
            !!document.querySelector("#thread .md code") &&
            !!document.querySelector("#thread .md li"),
          inlineImage: !!document.querySelector(
            '#thread [data-media="Image"] img',
          ),
          preimageImage: !!document.querySelector("#thread [data-preimage]"),
          video: !!document.querySelector('#thread [data-media="Video"]'),
          file: !!document.querySelector(
            '#thread [data-media="File"] a[download]',
          ),
          actions: !!document.querySelector(
            '#thread [data-body="Actions"] [data-action="act-yes"]',
          ),
          custom: !!document.querySelector('#thread [data-body="Custom"]'),
        }));
        return Object.values(found).every(Boolean) ? found : null;
      },
      "every body kind",
      15_000,
    );
    return `${await chat.shot("s07-host.png")}; ${Object.keys(kinds).join(", ")}`;
  });

  await scenario(rows, byId.S48, async () => {
    // The list derives its text from the newest message only while the
    // product sets no preview text, which an earlier run may have left set.
    await chat.command("host", "/preview typing off");
    await chat.command("host", "/post custom");
    await chat.until(async () => {
      const messages = await chat.messages();
      const last = messages
        .filter((message) => message.origin === "product")
        .at(-1);
      return last?.body === "Custom" && last.text.includes("Sample card");
    }, "a Custom bubble with its preview text");
    await chat.until(
      async () =>
        (
          await page.$eval('[data-room-id="host"]', (node) => node.innerText)
        ).includes("Sample card"),
      "the preview text in the chat list",
    );
    return `${await chat.shot("s48-host.png")}; the bubble and the list show "Sample card"`;
  });

  await scenario(rows, byId.S13, async () => {
    await chat.command("host", "/post actions");
    const target = await chat.until(async () => {
      const messages = await chat.messages();
      return messages.filter((message) => message.body === "Actions").at(-1);
    }, "an Actions message");
    const since = chat.log.length;
    await page.click(
      `[data-message-id="${target.id}"] [data-action="act-yes"]`,
    );
    const event = await chat.logLine(
      (line) =>
        line.via === "action_subscribe" && line.kind === "ActionTriggered",
      since,
    );
    if (
      event.data.messageId !== target.id ||
      event.data.actionId !== "act-yes"
    ) {
      throw new Error(
        `ActionTriggered named ${event.data.messageId}/${event.data.actionId}`,
      );
    }
    await chat.until(async () => {
      const buttons = await page.$$(
        `[data-message-id="${target.id}"] [data-action]`,
      );
      const edited = await page.$(
        `[data-message-id="${target.id}"][data-edited="true"]`,
      );
      return buttons.length === 0 && edited !== null;
    }, "the buttons removed by the edit");
    return `${await chat.shot("s13-host.png")}; ${target.id} pressed, edited without buttons`;
  });

  await scenario(rows, byId.S8, async () => {
    const before = (await chat.messages()).length;
    await chat.command("host", "/stream");
    const streamed = await chat.until(
      async () => {
        const messages = await chat.messages();
        const last = messages
          .filter((message) => message.origin === "product")
          .at(-1);
        return last &&
          last.edited &&
          last.status === "Sent" &&
          messages.length === before + 2
          ? last
          : null;
      },
      "one bubble streamed in place",
      15_000,
    );
    await chat.command(
      "host",
      `/edit ${streamed.id} Edited by the CLI battery`,
    );
    await chat.until(async () => {
      const messages = await chat.messages();
      const message = messages.find((entry) => entry.id === streamed.id);
      return (
        message?.text.includes("Edited by the CLI battery") && message.edited
      );
    }, "the edit in place");
    return `${await chat.shot("s08-host.png")}; ${streamed.id} streamed and edited in place`;
  });

  await scenario(rows, byId.S4, async () => {
    const row = (selector) => page.$(`[data-room-id="host"]${selector}`);
    const steps = [
      [
        "/preview pin",
        async () =>
          (await row('[data-pinned="true"]')) !== null &&
          (await page.$eval(
            "#rooms [data-room-id]",
            (node) => node.dataset.roomId,
          )) === "host",
      ],
      [
        "/preview badge 3",
        async () => (await row(' [data-badge="3"]')) !== null,
      ],
      [
        "/preview image",
        async () => (await row(' [data-badge="image"] img')) !== null,
      ],
      ["/preview none", async () => (await row(" [data-badge]")) === null],
      [
        "/preview typing on",
        async () => (await row(' [data-ongoing="true"]')) !== null,
      ],
      [
        "/preview typing off",
        async () => (await row(' [data-ongoing="true"]')) === null,
      ],
      [
        "/preview text Shown in the list",
        async () => (await row(' [data-preview="Shown in the list"]')) !== null,
      ],
      [
        "/preview unpin",
        async () => (await row('[data-pinned="true"]')) === null,
      ],
    ];
    const shots = [];
    for (const [command, check] of steps) {
      await chat.command("host", command);
      await chat.until(check, `the list after ${command}`);
      if (command === "/preview typing on" || command === "/preview image") {
        shots.push(
          await chat.shot(`s04-${command.split(" ").slice(1).join("-")}.png`),
        );
      }
    }
    return `${shots.join(", ")}; ${steps.length} preview variants drawn`;
  });

  return rows;
}

async function main() {
  mkdirSync(OUT_DIR, { recursive: true });
  for (const stale of ["diagnosis-report.md", "console.log"])
    rmSync(join(OUT_DIR, stale), { force: true });
  let host;
  let url = givenUrl;
  if (!url) {
    buildProduct();
    const port = Number(option("--port") ?? (await freePort()));
    host = await startHost(port);
    url = `http://127.0.0.1:${port}/chat`;
    // A watchdog that stops the launcher stops the host it started too.
    for (const signal of ["SIGINT", "SIGTERM"]) {
      process.once(signal, () => {
        host.kill("SIGKILL");
        process.exit(1);
      });
    }
  }
  const browser = await loadChromium().launch();
  let rows = [];
  let chat;
  try {
    const page = await browser.newPage({
      viewport: { width: 1400, height: 900 },
    });
    chat = new ChatPage(page);
    await page.goto(`${url}?room=source`);
    rows = await drive(chat);
  } finally {
    if (chat)
      writeFileSync(
        join(OUT_DIR, "console.log"),
        chat.console.join("\n") + "\n",
      );
    await browser.close();
    if (host) {
      host.kill("SIGTERM");
      await sleep(500);
      if (host.exitCode === null) host.kill("SIGKILL");
    }
  }
  writeFileSync(
    REPORT,
    renderScenarioReport("Truapi CLI Chat Page Diagnosis", rows),
  );
  console.log(`cli chat: report saved to ${relative(ROOT, REPORT)}`);
  const failures = scenarioFailures(rows);
  if (failures.length > 0) {
    throw new Error(
      `cli chat: ${failures.length} scenario row(s) failed or are missing:\n${failures.join("\n")}`,
    );
  }
  console.log(`cli chat: ${rows.length} scenarios passed`);
}

main().then(
  () => process.exit(0),
  (error) => {
    console.error(error?.stack ?? String(error));
    process.exit(1);
  },
);
