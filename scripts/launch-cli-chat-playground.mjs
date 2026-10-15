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
import {
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
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

/**
 * Build the product for `PRODUCT_ID`, so the room links it sends name the
 * product the host runs. A bundle built for another name is built again.
 */
function buildProduct() {
  const built =
    existsSync(WORKER_BUNDLE) &&
    readFileSync(WORKER_BUNDLE, "utf8").includes(JSON.stringify(PRODUCT_ID));
  if (built && !process.argv.includes("--build")) return;
  for (const args of [["install", "--frozen-lockfile"], ["build"]]) {
    const result = spawnSync("yarn", args, {
      cwd: PRODUCT_DIR,
      env: { ...process.env, CHAT_V2_DOTNS_NAME: PRODUCT_ID },
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

  /** Wait for the newest product message to match `text`, and give it. */
  async lastProductText(text, timeoutMs = 10_000) {
    const matches = (value) =>
      typeof text === "string" ? value === text : text.test(value);
    return this.until(
      async () => {
        const last = (await this.messages())
          .filter((message) => message.origin === "product")
          .at(-1);
        return last && matches(last.text.split("\n")[0].trim()) ? last : null;
      },
      `a product message ${text}`,
      timeoutMs,
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

  // S49: the product reads from `capabilities` exactly the features the page
  // lists, and both attachment bounds.
  await scenario(rows, byId.S49, async () => {
    const line = await page.$eval("#capabilities", (node) => node.innerText);
    const listed = line.split(" · ")[0].split(", ").filter(Boolean);
    await chat.command("host", "/caps");
    const answer = await chat.until(async () => {
      const last = (await chat.messages())
        .filter((message) => message.origin === "product")
        .at(-1);
      return last?.text.includes("caps:") ? last.text : null;
    }, "the capabilities message");
    const match = /caps: (.*); inline (\d+) B; preimage (\d+) B/.exec(answer);
    if (!match) throw new Error(`no capabilities in "${answer}"`);
    const read = match[1] === "none" ? [] : match[1].split(", ");
    if (read.join(",") !== listed.join(","))
      throw new Error(
        `the product read ${read.join(",")}, the page lists ${listed.join(",")}`,
      );
    if (match[2] !== String(1024 * 1024) || match[3] !== String(8 * 1024 * 1024))
      throw new Error(`bounds inline ${match[2]} B, preimage ${match[3]} B`);
    return `${await chat.shot("s49-host.png")}; ${read.length} features, inline ${match[2]} B, preimage ${match[3]} B`;
  });

  // S51: the host keeps the features of a room whole across a worker
  // restart, and marks a feature it does not serve without drawing it.
  await scenario(rows, byId.S51, async () => {
    const features = async () =>
      (await page.$("#roomfeatures"))
        ? await page.$eval("#roomfeatures", (node) => node.innerText)
        : "";
    await chat.command("host", "/features readaloud on");
    await chat.command("host", "/features calls audio");
    const declared =
      "features: calls audio (not served), read aloud (not served)";
    await chat.until(
      async () => (await features()) === declared,
      `"${declared}"`,
    );
    // Reloading the page restarts the worker; the host keeps the room.
    await page.click("#restart");
    await page.waitForSelector('[data-room-id="host"]', { timeout: 30_000 });
    await chat.until(
      async () =>
        (await page.$eval("#worker", (node) => node.innerText)).includes(
          "1 connected",
        ),
      "the worker after the restart",
      30_000,
    );
    await chat.room("host");
    await chat.until(
      async () => (await features()) === declared,
      `"${declared}" after the restart`,
    );
    await chat.command("host", "/features readaloud off");
    await chat.command("host", "/features calls off");
    await chat.until(
      async () => (await features()) === "",
      "no declared feature",
    );
    return `${await chat.shot("s51-host.png")}; ${declared} kept across the restart`;
  });

  // S53: a persona draws with its name and icon and no participants query.
  await scenario(rows, byId.S53, async () => {
    await chat.command("host", "/bot add helper Helper");
    await chat.lastProductText(/^bot helper (New|Exists)$/);
    await chat.command("host", "/bot post helper Hello from the persona");
    const spoken = await chat.until(async () => {
      const messages = await chat.messages();
      return messages
        .filter((message) => message.origin === "bot:helper")
        .find((message) => message.text.includes("Hello from the persona"));
    }, "the persona message");
    const drawn = async () =>
      page.$eval(
        `[data-message-id="${spoken.id}"]`,
        (node) => ({
          who: node.querySelector(".who")?.innerText ?? "",
          icon: !!node.querySelector('[data-persona="helper"] img'),
        }),
      );
    await chat.until(
      async () => {
        const { who, icon } = await drawn();
        return who === "Helper" && icon;
      },
      "the persona name and icon",
    );
    const asked = chat.log.filter(
      (line) => line.via === "participants_query" && line.room === "host",
    );
    if (asked.length > 0)
      throw new Error(`the page asked participants_query ${asked.length} time(s)`);
    await chat.command("host", "/bot add helper Helper Two");
    await chat.lastProductText("bot helper Exists");
    await chat.until(
      async () => (await drawn()).who === "Helper Two",
      "the replaced persona name",
    );
    await chat.command("host", "/bot typing helper");
    await chat.until(
      async () =>
        (await page.$eval("#activity", (node) => node.innerText)).startsWith(
          "Helper Two: ",
        ),
      "the activity line of the persona",
    );
    const shot = await chat.shot("s53-host.png");
    await chat.command("host", "/activity off");
    await chat.command("host", "/bot post ghost Boo");
    await chat.lastProductText(/unknown bot ghost/);
    return `${shot}; ${spoken.id} drawn as Helper with its icon, renamed to Helper Two, ghost refused`;
  });

  // S54: a muted room greys its badge and shows no notification of the
  // product; the product hears UserStateChanged.
  await scenario(rows, byId.S54, async () => {
    const roomEvent = (since, muted) =>
      chat.logLine(
        (line) =>
          line.via === "room_events" &&
          line.room === "host" &&
          line.kind === "UserStateChanged" &&
          (line.data.mutedUntil !== null) === muted,
        since,
      );
    const shown = () =>
      page.$$eval('[data-notification][data-room="host"]', (nodes) => nodes.length);
    await chat.command("host", "/preview badge 2");
    let since = chat.log.length;
    await page.click("#mute");
    await page.click('[data-item="mute-1-hour"]');
    await roomEvent(since, true);
    await chat.until(
      async () =>
        (await page.$('[data-room-id="host"][data-muted="true"] .badge.muted')) !== null,
      "the muted row with a grey badge",
    );
    const before = await shown();
    await chat.command("host", "/notify");
    await chat.lastProductText("ok /notify");
    await sleep(1_000);
    if ((await shown()) !== before)
      throw new Error("a notification into the muted room was shown");
    const muted = await chat.shot("s54-muted.png");
    since = chat.log.length;
    await page.click("#mute");
    await roomEvent(since, false);
    await chat.command("host", "/notify");
    await chat.until(
      async () => (await shown()) === before + 1,
      "the notification once the room is unmuted",
    );
    await chat.command("host", "/preview none");
    return `${muted}, ${await chat.shot("s54-unmuted.png")}; no notification while muted, one after`;
  });

  // S55: an archived room stays archived when a message arrives.
  await scenario(rows, byId.S55, async () => {
    const archivedRow = () => page.$("#archived");
    const since = chat.log.length;
    await page.click("#archive");
    await chat.logLine(
      (line) =>
        line.via === "room_events" &&
        line.room === "host" &&
        line.data.archived === true,
      since,
    );
    await chat.until(async () => (await archivedRow()) !== null, "the Archived row");
    await chat.command("host", "/post text");
    await chat.lastProductText(/^Plain text message/);
    if ((await page.$('#rooms > [data-room-id="host"]:not([data-archived])')) !== null)
      throw new Error("the room left the archived list when a message arrived");
    await page.click("#archived");
    await chat.until(
      async () =>
        (await page.$('[data-room-id="host"][data-archived="true"]')) !== null,
      "the room in the archived list",
    );
    const shot = await chat.shot("s55-archived.png");
    await page.click("#archive");
    await chat.until(
      async () =>
        (await archivedRow()) === null &&
        (await page.$('[data-room-id="host"]:not([data-archived])')) !== null,
      "the room back in the list",
    );
    return `${shot}; archived through /post text, unarchived`;
  });

  // S56: the snapshot of the user state comes first on the room events of
  // a restarted worker.
  await scenario(rows, byId.S56, async () => {
    await page.click("#mute");
    await page.click('[data-item="mute-always"]');
    await chat.room("source");
    await page.click("#archive");
    await chat.until(async () => (await page.$("#archived")) !== null, "source archived");
    const boot = Math.max(...chat.log.map((line) => line.boot));
    await page.click("#restart");
    await page.waitForSelector('[data-room-id="host"]', { timeout: 30_000 });
    const lines = await chat.until(
      async () => {
        const events = chat.log.filter(
          (line) => line.boot > boot && line.via === "room_events",
        );
        return events.length >= 2 ? events : null;
      },
      "the room events of the restarted worker",
      30_000,
    );
    const first = lines.slice(0, 2).map((line) => `${line.room} ${line.kind}`);
    if (first.join(",") !== "host UserStateChanged,source UserStateChanged")
      throw new Error(`the restarted worker heard ${first.join(", ")} first`);
    if (lines[0].data.mutedUntil === null || lines[1].data.archived !== true)
      throw new Error(`snapshot ${JSON.stringify(lines.slice(0, 2).map((line) => line.data))}`);
    // Back to a plain start for the next run.
    await chat.room("host");
    await page.click("#mute");
    await page.click("#archived");
    await chat.room("source");
    await page.click("#archive");
    await chat.until(async () => (await page.$("#archived")) === null, "no archived room");
    await chat.room("host");
    return `the restarted worker (boot ${boot + 1}) heard ${first.join(", ")} first`;
  });

  // S57 needs a Bulletin chain that stores 3 MiB and 6 MiB in one
  // transaction. The dev page stages on the live chain, whose
  // `MaxTransactionSize` is 2 MiB, so the core refuses those sizes there
  // (open item O32). The test host, which keeps preimages in the core, runs
  // S57; here the row is a skip with that reason.
  const MIB = 1024 * 1024;
  const upload = async (input, name, mimeType, size) => {
    const buffer = Buffer.alloc(size);
    for (let index = 0; index < size; index++) buffer[index] = index % 251;
    await page.setInputFiles(input, { name, mimeType, buffer });
  };
  rows.push({
    ...byId.S57,
    result: "skip",
    durationMs: 0,
    evidence:
      "the live Bulletin chain stores at most 2 MiB in one transaction (O32); S57 runs on the test host",
  });
  console.log(`skip ${byId.S57.id} ${byId.S57.title}`);

  // S58: no declaration, or a file above 8 MiB, uploads and sends nothing.
  await scenario(rows, byId.S58, async () => {
    await chat.room("host");
    await chat.command("host", "/footer photo");
    await chat.command("host", "/features preimage off");
    const status = () => page.$eval("#status", (node) => node.innerText);
    const since = chat.log.length;
    await upload("#attach-file", "undeclared.pdf", "application/pdf", 3 * MIB);
    await chat.until(
      async () => /not sent: .*does not declare preimage attachments/.test(await status()),
      "the refusal of an undeclared room",
    );
    const undeclared = await status();
    await chat.command("host", "/features preimage on");
    await upload("#attach-file", "huge.pdf", "application/pdf", 8 * MIB + 1);
    await chat.until(
      async () => /not sent: huge\.pdf .* above 8\.0 MB/.test(await status()),
      "the refusal above 8 MiB",
    );
    const huge = await status();
    await sleep(500);
    const sent = chat.log.slice(since).filter((entry) => entry.via === "media");
    if (sent.length > 0) throw new Error(`the product received ${sent.length} media item(s)`);
    await chat.command("host", "/features preimage off");
    await chat.command("host", "/footer input");
    return `"${undeclared}"; "${huge}"`;
  });

  const searchFor = async (text) => {
    await page.fill("#searchtext", text);
    return chat.until(
      async () => {
        const state = await page.$eval("#searchresults", (node) => ({
          answered: node.dataset.answered,
          unavailable: node.dataset.unavailable === "true",
          hits: [...node.querySelectorAll("[data-hit]")].map((hit) => hit.dataset.hit),
        }));
        return state.answered === text ? state : null;
      },
      `the results for "${text}"`,
    );
  };

  // S61: the host searches a room it keeps itself.
  await scenario(rows, byId.S61, async () => {
    await chat.room("host");
    await chat.type("Crème brûlée for the search");
    await chat.until(
      async () => (await chat.messages()).some((message) => message.text.includes("Crème brûlée")),
      "the message to find",
    );
    await page.click("#search-toggle");
    const found = await searchFor("CREME BRULEE");
    if (found.hits.length === 0) throw new Error("no match with case and diacritics ignored");
    await page.click(`[data-hit="${found.hits[0]}"]`);
    await chat.until(
      async () => (await page.$(`[data-message-id="${found.hits[0]}"].flash`)) !== null,
      "the jump to the match",
    );
    const asked = chat.log.filter((line) => line.via === "search_query");
    if (asked.length > 0) throw new Error("the host asked the product to search its own store");
    const shot = await chat.shot("s61-host.png");
    await page.click("#search-toggle");
    return `${shot}; ${found.hits.length} match(es), jumped to ${found.hits[0]}`;
  });

  // S62: a room the product keeps asks the product, and shows search only
  // when it declares it.
  await scenario(rows, byId.S62, async () => {
    await chat.room("source");
    await chat.command("source", "/history 30");
    await chat.reopen("source", "host");
    await chat.command("source", "/search on");
    await chat.until(async () => (await page.$("#search-toggle")) !== null, "the search action");
    await page.click("#search-toggle");
    const since = chat.log.length;
    const found = await searchFor("message 0001");
    const expected = Array.from({ length: 10 }, (_, index) => `source-h-${String(19 - index).padStart(5, "0")}`);
    if (found.hits.join(",") !== expected.join(","))
      throw new Error(`hits ${found.hits.join(", ")}`);
    await chat.logLine((line) => line.via === "search_query" && line.room === "source", since);
    await page.click('[data-hit="source-h-00012"]');
    await chat.until(
      async () => (await page.$('[data-message-id="source-h-00012"].flash')) !== null,
      "the jump to source-h-00012",
    );
    const shot = await chat.shot("s62-source.png");
    await chat.command("source", "/search na");
    const na = await searchFor("message 0002");
    if (!na.unavailable) throw new Error("search na did not show not available");
    await chat.command("source", "/search off");
    await chat.until(async () => (await page.$("#search-toggle")) === null, "no search action without search");
    return `${shot}; ${found.hits.length} hits from search_query, jumped to source-h-00012; na not available; off hides search`;
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
  const skipped = rows.filter((row) => row.result === "skip").length;
  console.log(
    `cli chat: ${rows.length - skipped} scenarios passed, ${skipped} skipped`,
  );
}

main().then(
  () => process.exit(0),
  (error) => {
    console.error(error?.stack ?? String(error));
    process.exit(1);
  },
);
