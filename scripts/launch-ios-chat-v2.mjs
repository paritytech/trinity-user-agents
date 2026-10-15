#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Run the Chat Extension v2 sample product (`playground/chat-v2`) in the
// iPhone simulator app, and write one diagnosis row for each scenario of the
// test plan that names the iOS layer.
//
// The launcher builds the product, installs its worker into the app's
// product storage, and starts the app with the simulator-only E2E hooks: a
// seeded wallet and lite username (no onboarding taps), the product as a chat
// bot, its `host` room open, and a command feed. It then goes through the
// phases of each scenario: it sends the commands of the scenario to the
// product and the actions of the user to the app's harness, checks Core Data,
// the event log of the product and the `[e2e:ui]` markers of the app, and
// takes screenshots. A scenario the launcher cannot drive gets a `skip` row
// that says why. The report
// goes to `playground/test-results/ios-chat-v2/diagnosis-report.md`, with the
// screenshots and the app's console beside it.
//
// The launcher fails when a driven scenario fails or has no row.

import { spawn } from "node:child_process";
import {
  cpSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname, relative, resolve } from "node:path";

import {
  renderScenarioReport,
  scenarioFailures,
} from "./lib/cli-chat-scenarios.mjs";
import { decodeTextMessage } from "./lib/chat-diagnosis-report.mjs";
import {
  IOS_CHAT_V2_PERF,
  IOS_CHAT_V2_SCENARIOS,
  appStartFailure,
  blockedRows,
  capabilityVerdict,
  eventMatches,
  lastProductMessage,
  lastUserMessage,
  parseEventLog,
  parseUiMarkers,
  row,
  skipRow,
} from "./lib/ios-chat-v2-scenarios.mjs";
import {
  DEFAULT_BUNDLE,
  E2E_SEED_IDENTITY,
  appGroupId,
  bootAndInstallApp,
  defaultAppPath,
  readPlistValue,
  userDataDatabase,
} from "./lib/ios-simulator.mjs";
import {
  capture,
  captureOptional,
  delay,
  run,
  runAsync,
  waitFor,
} from "./lib/process.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const bundle = process.env.TRUAPI_IOS_E2E_BUNDLE ?? DEFAULT_BUNDLE;
const app = process.env.TRUAPI_IOS_E2E_APP ?? defaultAppPath(repoRoot);
const productRoot = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_DIR ?? "playground/chat-v2",
);
const productHost =
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST ?? "truapi-chat-v2.dot";
const productName =
  process.env.TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME ?? "Chat Extension v2 Sample";
const seedIdentity =
  process.env.TRUAPI_IOS_E2E_SEED_IDENTITY ?? E2E_SEED_IDENTITY;
const outDir = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_CHAT_V2_OUT ??
    "playground/test-results/ios-chat-v2",
);
const reportPath = resolve(outDir, "diagnosis-report.md");
const appLogPath = resolve(outDir, "app.log");
const worker = resolve(productRoot, "out/worker/index.js");
const room = "host";
const chatIdentifier = `1:${productHost}:${room}`;
/** Names the command files in the order they are queued. */
let commandCounter = 0;

/** The actions of the harness that move to another screen. */
const NAVIGATIONS = [
  "open",
  "list",
  "back",
  "participants",
  "archived",
  "search-open",
  "search-select",
];

/** `TRUAPI_IOS_E2E_ONLY=S3,S4` drives these scenarios only, to work on one row. */
const only = (process.env.TRUAPI_IOS_E2E_ONLY || undefined)
  ?.split(",")
  .map((id) => id.trim());

/**
 * `TRUAPI_IOS_E2E_PERF=1` also drives the performance rows B1 to B3 after the
 * scenarios (P7.1).
 */
const scenarios =
  process.env.TRUAPI_IOS_E2E_PERF === "1"
    ? [...IOS_CHAT_V2_SCENARIOS, ...IOS_CHAT_V2_PERF]
    : IOS_CHAT_V2_SCENARIOS;

/** The values that a step filled in, for the expectations after it. */
const memory = {};

/** How long the app has to start the product and create its room. */
const START_TIMEOUT_MS = 120_000;
/** How long one command has to reach the product and take effect. */
const COMMAND_TIMEOUT_MS = 20_000;

/**
 * Prepended to the installed worker. The app's console bridge forwards
 * `console.log`, `warn` and `error`, and writes an `Error` as `{}`. The event
 * log of the sample product uses `console.info`, so the shim sends it to
 * `console.log`, and writes each `Error` as its name and message.
 */
const CONSOLE_SHIM = `{
  const text = (value) => value instanceof Error ? value.name + ": " + value.message : value;
  const log = console.log.bind(console);
  const error = console.error.bind(console);
  console.info = (...values) => log(...values.map(text));
  console.error = (...values) => error(...values.map(text));
}
`;

if (!existsSync(app)) {
  throw new Error(`iOS app bundle not found: ${app}`);
}
if (!existsSync(resolve(productRoot, "package.json"))) {
  throw new Error(`Chat product source not found: ${productRoot}`);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

// The DotNS name goes into the room links and the deeplink of the product.
const productBuild =
  process.env.TRUAPI_IOS_E2E_SKIP_PRODUCT_BUILD !== "1"
    ? runAsync("yarn", ["build"], {
        cwd: productRoot,
        env: { ...process.env, CHAT_V2_DOTNS_NAME: productHost },
      })
    : Promise.resolve();
productBuild.catch(() => {});

const device = bootAndInstallApp(app);
await productBuild;
if (!existsSync(worker)) {
  throw new Error(`Chat product worker not found after build: ${worker}`);
}

const appData = capture("xcrun", [
  "simctl",
  "get_app_container",
  device.udid,
  bundle,
  "data",
]).trim();
const appGroup = capture("xcrun", [
  "simctl",
  "get_app_container",
  device.udid,
  bundle,
  appGroupId(bundle),
]).trim();
installWorker();
// The markers of an earlier run would stop the app's start hooks early.
rmSync(resolve(appData, "tmp/truapi-e2e"), { recursive: true, force: true });
const commandDir = resolve(appData, "tmp/truapi-e2e/commands");
rmSync(commandDir, { recursive: true, force: true });
mkdirSync(commandDir, { recursive: true });

// The attachments of S36: an image under the inline limit, and a file over it.
const fileDir = resolve(appData, "tmp/truapi-e2e/files");
rmSync(fileDir, { recursive: true, force: true });
mkdirSync(fileDir, { recursive: true });
cpSync(
  resolve(productRoot, "assets/sample.png"),
  resolve(fileDir, "sample.png"),
);
writeFileSync(resolve(fileDir, "large.bin"), Buffer.alloc(2 * 1024 * 1024, 7));

const appLog = createWriteStream(appLogPath);
const appConsole = spawn(
  "xcrun",
  [
    "simctl",
    "launch",
    "--console-pty",
    "--terminate-running-process",
    device.udid,
    bundle,
  ],
  {
    env: {
      ...process.env,
      SIMCTL_CHILD_RUST_BACKTRACE: "1",
      SIMCTL_CHILD_TRUAPI_IOS_E2E_SEED_IDENTITY: seedIdentity,
      SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST: productHost,
      SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME: productName,
      SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_ROOM_ID: room,
      SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_COMMANDS: "1",
      SIMCTL_CHILD_TRUAPI_IOS_E2E_RUNTIME_MARKERS: "1",
      SIMCTL_CHILD_TRUAPI_IOS_E2E_NO_PUSH_PROMPT: "1",
      // `/notify` needs the notification permission, and the harness cannot
      // answer the system prompt (S54).
      SIMCTL_CHILD_TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
  },
);
let consoleText = "";
/** The app ended, for example on a crash: every later step fails at once. */
let appEnded = false;
appConsole.on("exit", () => {
  appEnded = true;
});
for (const stream of [appConsole.stdout, appConsole.stderr]) {
  stream.on("data", (chunk) => {
    consoleText += chunk.toString();
    appLog.write(chunk);
  });
}

const rows = [];
let failure;
try {
  const started = await waitForStart();
  if (typeof started === "string") {
    // Give the screen time to show the failure, for the evidence.
    await delay(5_000);
    run(
      "xcrun",
      ["simctl", "io", device.udid, "screenshot", resolve(outDir, "start.png")],
      {
        stdio: "ignore",
      },
    );
    rows.push(...blockedRows(`${started} (\`start.png\`)`, scenarios));
    failure = started;
  } else {
    // The room screen of the `host` room is where the scenarios start.
    await sendHostAction({ room, host: "open" });
    await delay(2_000);
    for (const scenario of scenarios) {
      rows.push(await runScenario(scenario));
    }
  }
} catch (error) {
  failure = error instanceof Error ? error.message : String(error);
  for (const scenario of scenarios) {
    if (!rows.some((existing) => existing.id === scenario.id)) {
      rows.push(
        scenario.drive
          ? row(scenario, "fail", 0, `not run: ${failure}`)
          : skipRow(scenario),
      );
    }
  }
} finally {
  captureOptional("xcrun", ["simctl", "terminate", device.udid, bundle]);
  appConsole.kill();
  appLog.end();
}

const events = parseEventLog(consoleText);
writeFileSync(
  resolve(outDir, "event-log.jsonl"),
  events.map((entry) => JSON.stringify(entry)).join("\n"),
);
const report = renderScenarioReport(
  "Truapi iOS Chat Extension v2 Diagnosis",
  rows,
);
const notes = [
  "",
  `Device: ${device.name} (${device.udid}). Product: \`${productHost}\`, room \`${room}\`.`,
  `Event log: ${events.length} lines in \`event-log.jsonl\`; app console in \`app.log\`.`,
  ...(failure ? ["", `**The run stopped:** ${failure}`] : []),
];
writeFileSync(reportPath, `${report}${notes.join("\n")}\n`);

const failures = scenarioFailures(
  rows,
  scenarios.filter((scenario) => scenario.drive),
);
console.log(
  JSON.stringify({
    device: device.name,
    deviceId: device.udid,
    productHost,
    report: relative(repoRoot, reportPath),
    rows: rows.length,
    passed: rows.filter((entry) => entry.result === "pass").length,
    failures,
  }),
);
if (failures.length > 0) process.exit(1);

/**
 * Copy the worker, with the console shim, to where the app reads a product
 * that has no published worker, and to the DotNS cache of the product when
 * one exists.
 */
function installWorker() {
  const destinations = [
    resolve(
      appData,
      "Library/Application Support/Products",
      productHost,
      "ChatExtension/index.js",
    ),
  ];
  const contentHash = readPlistValue(
    resolve(appData, "Library/Preferences/io.products.dotns.cache.plist"),
    productHost,
  );
  if (contentHash && /^[0-9a-f]+$/i.test(contentHash)) {
    destinations.push(
      resolve(
        appData,
        "Library/Application Support/DotNsContent",
        contentHash,
        "worker/index.js",
      ),
    );
  }
  const source = `${CONSOLE_SHIM}${readFileSync(worker, "utf8")}`;
  for (const destination of destinations) {
    mkdirSync(dirname(destination), { recursive: true });
    writeFileSync(destination, source);
  }
}

/**
 * Wait until the product has created its `host` room, or until the console
 * shows why the app cannot run it. Gives a reason when the app cannot.
 */
async function waitForStart() {
  return waitFor(
    () => {
      const reason = appStartFailure(consoleText);
      if (reason) return reason;
      // The room can be in Core Data from an earlier run, so the bot of the product must have started too.
      const botStarted = consoleText.includes(
        `Rust chat runtime started for: ${productHost}`,
      );
      return roomName() && botStarted ? true : undefined;
    },
    {
      timeoutMs: START_TIMEOUT_MS,
      message: () =>
        `Timed out waiting for the product to create ${chatIdentifier}; see ${relative(repoRoot, appLogPath)}`,
    },
  );
}

async function runScenario(scenario) {
  if (only && !only.includes(scenario.id)) {
    return row(scenario, "skip", 0, "skip: not in TRUAPI_IOS_E2E_ONLY");
  }
  if (!scenario.drive) return skipRow(scenario);
  const result = await driveScenario(scenario);
  return scenario.capabilities ? capabilityVerdict(result, rows) : result;
}

/**
 * Run the phases of a scenario. Each phase sends its steps, then waits for
 * each expectation in the console, the event log or Core Data. The room is
 * put back with `after` whether the scenario passed or not.
 */
async function driveScenario(scenario) {
  const started = Date.now();
  const room = scenario.mode;
  const baseline = { contentTypes: contentTypeCounts(room) };
  const evidence = [];
  const shots = [];
  // What an expectation with `scope: "previous"` reads from: the console and
  // the event log since the previous phase started, not since its own phase.
  let previous;
  try {
    for (const [index, phase] of scenario.drive.phases.entries()) {
      const mark = {
        started: Date.now(),
        console: consoleText.length,
        events: parseEventLog(consoleText).length,
        reactions: reactionCount(room),
      };
      for (const step of phase.steps) {
        await runStep(step, room);
      }
      for (const expectation of phase.expect) {
        evidence.push(
          await check(expectation, {
            room,
            mark: expectation.scope === "previous" ? (previous ?? mark) : mark,
            baseline,
          }),
        );
      }
      if (phase.shot || index === scenario.drive.phases.length - 1) {
        shots.push(await takeScreenshot(scenario, phase.shot));
      }
      previous = mark;
    }
    return row(
      scenario,
      "pass",
      Date.now() - started,
      `${shots.map((shot) => `\`${shot}\``).join(", ")}; ${evidence.join("; ")}`,
    );
  } catch (error) {
    const screenshot = await takeScreenshot(scenario, "fail").catch(
      () => undefined,
    );
    const shot = screenshot ? `\`${screenshot}\`; ` : "";
    return row(
      scenario,
      "fail",
      Date.now() - started,
      `${shot}${error.message}${evidence.length ? `; before that: ${evidence.join("; ")}` : ""}`,
    );
  } finally {
    for (const step of scenario.drive.after ?? []) {
      await runStep(step, room).catch((error) =>
        console.error(`${scenario.id} after: ${error.message}`),
      );
    }
  }
}

function checkAppRuns() {
  if (
    appEnded ||
    /Terminating app due to uncaught exception/.test(consoleText)
  ) {
    throw new Error("the app ended; see app.log");
  }
}

async function runStep(step, room) {
  checkAppRuns();
  if (typeof step === "string") return sendCommand(fill(step, room), room);
  if (step.wait) return delay(step.wait);
  if (step.text) {
    const target = step.room ? fill(step.room, room) : room;
    return sendCommand(fill(step.text, target), target);
  }
  if (step.host) {
    const target = step.room ? fill(step.room, room) : room;
    await sendHostAction({
      room: target,
      host: step.host,
      argument:
        step.argument === undefined ? undefined : fill(step.argument, target),
      target: step.target === undefined ? undefined : fill(step.target, target),
    });
    // The screen needs a moment to take a navigation.
    if (NAVIGATIONS.includes(step.host)) {
      await delay(1_500);
    }
    return;
  }
  throw new Error(`unknown step ${JSON.stringify(step)}`);
}

/** Fill the templates of the scenario library in a step or an expectation. */
function fill(text, room) {
  return text
    .replaceAll(/\{memory:(\w+)\}/g, (_, key) => {
      if (!(key in memory)) throw new Error(`nothing remembered as ${key}`);
      return memory[key];
    })
    .replaceAll("{lastUser}", () => {
      const id = lastUserMessage(parseUiMarkers(consoleText), room);
      if (!id)
        throw new Error(`no message of the user in the window of ${room}`);
      return (memory.lastUser = id);
    })
    .replaceAll("{lastProduct}", () => {
      const id = lastProductMessage(parseUiMarkers(consoleText), room);
      if (!id)
        throw new Error(`no message of the product in the window of ${room}`);
      return (memory.lastProduct = id);
    })
    .replaceAll("{reacted}", () => {
      const id = sql(`
        SELECT message.ZMESSAGEID FROM ZCDCHATMESSAGEREACTION AS reaction
        JOIN ZCDCHATMESSAGE AS message ON message.Z_PK = reaction.ZMESSAGE
        JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
        WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(room))}
        ORDER BY reaction.ZTIMESTAMP DESC LIMIT 1;
      `);
      if (!id) throw new Error(`no reacted message in ${room}`);
      return id.split("/").at(-1);
    })
    .replaceAll(/\{file:([^}]+)\}/g, (_, name) => resolve(fileDir, name));
}

async function check(expectation, { room, mark, baseline }) {
  checkAppRuns();
  const timeoutMs = expectation.timeoutMs ?? COMMAND_TIMEOUT_MS;
  const target = expectation.room ? fill(expectation.room, room) : room;
  if (expectation.stored !== undefined) {
    const wanted = fill(expectation.stored, target);
    await waitFor(
      () =>
        messageTexts(target).some((text) =>
          expectation.prefix ? text.startsWith(wanted) : text === wanted,
        ),
      { timeoutMs, message: `no message "${wanted}" stored in ${target}` },
    );
    return `stored "${wanted}"`;
  }
  if (expectation.storedMatch !== undefined) {
    const pattern = new RegExp(fill(expectation.storedMatch, target));
    let found;
    await waitFor(
      () => {
        found = messageTexts(target).find((text) => pattern.test(text));
        return found;
      },
      {
        timeoutMs,
        message: `no message matching /${pattern.source}/ stored in ${target}`,
      },
    );
    return `stored "${found}"`;
  }
  if (expectation.storedText !== undefined) {
    const wanted = fill(expectation.storedText, target);
    await waitFor(() => roomDataContains(target, wanted), {
      timeoutMs,
      message: `no message holding "${wanted}" stored in ${target}`,
    });
    return `stored "${wanted}"`;
  }
  if (expectation.metadata || expectation.metadataHex) {
    const wanted = expectation.metadata ?? expectation.metadataHex;
    const columns = Object.keys(wanted);
    let found;
    await waitFor(
      () => {
        found = metadataRow(target, columns, {
          hex: Boolean(expectation.metadataHex),
        });
        return columns.every((column) => found?.[column] === wanted[column]);
      },
      {
        timeoutMs,
        message: () =>
          `room metadata ${JSON.stringify(found)} is not ${JSON.stringify(wanted)}`,
      },
    );
    return expectation.timed
      ? `metadata ${JSON.stringify(wanted)} after ${Date.now() - mark.started} ms`
      : `metadata ${JSON.stringify(wanted)}`;
  }
  if (expectation.gone) {
    await waitFor(() => !chatExists(target), {
      timeoutMs,
      message: `the room ${target} is still stored`,
    });
    return `room ${target} gone`;
  }
  if (expectation.contentType !== undefined) {
    const before = baseline.contentTypes[expectation.contentType] ?? 0;
    let now = before;
    await waitFor(
      () => {
        now = contentTypeCounts(target)[expectation.contentType] ?? 0;
        return now - before >= expectation.added;
      },
      {
        timeoutMs,
        message: () =>
          `${now - before} new messages of content type ${expectation.contentType}, not ${expectation.added}`,
      },
    );
    return `${now - before} new of content type ${expectation.contentType}`;
  }
  if (expectation.reactions !== undefined) {
    let now = mark.reactions;
    await waitFor(
      () => {
        now = reactionCount(target);
        return now - mark.reactions === expectation.reactions;
      },
      {
        timeoutMs,
        message: () =>
          `reaction rows changed by ${now - mark.reactions}, not ${expectation.reactions}`,
      },
    );
    return `reaction rows ${expectation.reactions > 0 ? "+" : ""}${expectation.reactions}`;
  }
  if (expectation.previewNot !== undefined) {
    await delay(expectation.settleMs ?? 1_000);
    const unwanted = fill(expectation.previewNot, target);
    if (previewContains(target, unwanted)) {
      throw new Error(`the chat-list preview of ${target} holds "${unwanted}"`);
    }
    return `preview without "${unwanted}"`;
  }
  if (expectation.ui) {
    const key = fill(expectation.ui, target);
    const pattern = new RegExp(fill(expectation.match, target));
    // A marker repeats only when its value changes, so the newest value is the state of the screen.
    const found = await waitFor(
      () => {
        const newest = parseUiMarkers(consoleText)
          .filter(
            (marker) => marker.key === key && marker.room === target,
          )
          .at(-1);
        return newest && pattern.test(newest.value) ? newest : undefined;
      },
      {
        timeoutMs,
        message: () => {
          const last = parseUiMarkers(consoleText)
            .filter(
              (marker) =>
                marker.key === key && marker.room === target,
            )
            .at(-1);
          return `no ${key} marker of ${target} matching /${pattern.source}/ (last: ${last?.value ?? "none"})`;
        },
      },
    );
    return `${key} "${found.value}"`;
  }
  if (expectation.uiNot) {
    await delay(expectation.settleMs ?? 1_000);
    const key = fill(expectation.uiNot, target);
    const pattern = new RegExp(fill(expectation.match, target));
    const last = parseUiMarkers(consoleText)
      .filter((marker) => marker.key === key && marker.room === target)
      .at(-1);
    if (last && pattern.test(last.value)) {
      throw new Error(`${key} marker "${last.value}" matches /${pattern.source}/`);
    }
    return `${key} "${last?.value ?? "none"}"`;
  }
  if (expectation.event) {
    const expected = {
      ...expectation.event,
      data: expectation.event.data && fill(expectation.event.data, target),
    };
    const found = await waitFor(
      () =>
        parseEventLog(consoleText)
          .slice(mark.events)
          .find((entry) => eventMatches(entry, expected, target)),
      {
        timeoutMs,
        message: `the product logged no ${JSON.stringify(expected)}`,
      },
    );
    return `event ${found.via} ${found.kind} #${found.seq}`;
  }
  if (expectation.consoleNot) {
    await delay(expectation.settleMs ?? 2_000);
    const written = consoleText.slice(mark.console);
    if (written.includes(expectation.consoleNot)) {
      throw new Error(`"${expectation.consoleNot}" in the console`);
    }
    return `no "${expectation.consoleNot}" in the console`;
  }
  if (expectation.console) {
    const wanted = fill(expectation.console, target);
    await waitFor(() => consoleText.slice(mark.console).includes(wanted), {
      timeoutMs,
      message: `no "${wanted}" in the console`,
    });
    return `console "${wanted}"`;
  }
  throw new Error(`unknown expectation ${JSON.stringify(expectation)}`);
}

/** Queue one file for the app's command feed, and wait until it is taken. */
async function queueCommand(payload, label) {
  commandCounter += 1;
  const name = String(commandCounter).padStart(4, "0");
  const pending = resolve(commandDir, `${name}.tmp`);
  writeFileSync(pending, JSON.stringify(payload));
  // A rename is atomic: the app never reads a half-written file, and a copy
  // could fail when the app takes the file before the copy sets its mode.
  renameSync(pending, resolve(commandDir, `${name}.json`));
  const outcome = await waitFor(
    () => {
      if (existsSync(resolve(commandDir, `${name}.done`))) return "done";
      const failed = resolve(commandDir, `${name}.failed`);
      return existsSync(failed)
        ? `failed: ${readFileSync(failed, "utf8")}`
        : undefined;
    },
    {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `the app did not take ${label}`,
    },
  );
  if (outcome !== "done") throw new Error(`${label} ${outcome}`);
}

/** Send a command to the product, and wait until it logs it. */
async function sendCommand(text, commandRoom = room) {
  const seen = parseEventLog(consoleText).length;
  await queueCommand({ room: commandRoom, text }, `\`${text}\``);
  // The product receives the command as a `MessagePosted` event.
  await waitFor(
    () =>
      parseEventLog(consoleText)
        .slice(seen)
        .some(
          (entry) =>
            entry.via === "action_subscribe" &&
            entry.kind === "MessagePosted" &&
            entry.room === commandRoom &&
            JSON.stringify(entry.data).includes(
              JSON.stringify(text).slice(1, -1),
            ),
        ),
    {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `the product did not log \`${text}\` as MessagePosted`,
    },
  );
  // Let the answer of the product reach the screen before the next step.
  await delay(400);
}

/** Ask the app's harness for an action of the user on a room screen. */
async function sendHostAction(action) {
  await queueCommand(action, `the ${action.host} action`);
}

async function takeScreenshot(scenario, suffix) {
  // The newest message goes on screen, when the room of the scenario shows.
  if (!appEnded) {
    await sendHostAction({ room: scenario.mode, host: "bottom" }).catch(
      () => {},
    );
  }
  // The bubble or the footer change needs a moment to reach the screen.
  await delay(1_500);
  const id = scenario.id.toLowerCase().replace(/^s(\d)$/, "s0$1");
  const name = `${id}-${scenario.mode}${suffix ? `-${suffix}` : ""}.png`;
  run(
    "xcrun",
    ["simctl", "io", device.udid, "screenshot", resolve(outDir, name)],
    {
      stdio: "ignore",
    },
  );
  return name;
}

function chatIdentifierOf(roomId) {
  return `1:${productHost}:${roomId}`;
}

function roomName() {
  return (
    sql(`
    SELECT metadata.ZNAME FROM ZCDCHAT AS chat
    JOIN ZCDCHATROOMMETADATA AS metadata ON metadata.ZCHAT = chat.Z_PK
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifier)};
  `) ||
    (sql(
      `SELECT 1 FROM ZCDCHAT WHERE ZIDENTIFIER = ${sqlString(chatIdentifier)};`,
    )
      ? "(no metadata)"
      : undefined)
  );
}

/** Columns of the metadata row of a room, as strings. */
function metadataRow(roomId, columns, { hex = false } = {}) {
  const value = sql(`
    SELECT ${columns.map((column) => (hex ? `hex(metadata.${column})` : `metadata.${column}`)).join(", ")}
    FROM ZCDCHAT AS chat
    JOIN ZCDCHATROOMMETADATA AS metadata ON metadata.ZCHAT = chat.Z_PK
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))};
  `);
  if (value === undefined) return undefined;
  const parts = value.split("|");
  return Object.fromEntries(
    columns.map((column, index) => [column, parts[index]]),
  );
}

/** True when the app stores the chat of a room. */
function chatExists(roomId) {
  const count = sql(`
    SELECT count(*) FROM ZCDCHAT AS chat
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))};
  `);
  return Number(count ?? 0) > 0;
}

/** The number of messages of each content type in a room. */
function contentTypeCounts(roomId) {
  const rows = sql(`
    SELECT message.ZCONTENTTYPE, count(*) FROM ZCDCHATMESSAGE AS message
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))}
    GROUP BY message.ZCONTENTTYPE;
  `);
  return Object.fromEntries(
    (rows ?? "")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => line.split("|").map(Number)),
  );
}

/** The number of reaction rows on the messages of a room. */
function reactionCount(roomId) {
  return Number(
    sql(`
    SELECT count(*) FROM ZCDCHATMESSAGEREACTION AS reaction
    JOIN ZCDCHATMESSAGE AS message ON message.Z_PK = reaction.ZMESSAGE
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))};
  `) ?? 0,
  );
}

function messageTexts(roomId = room) {
  const values = sql(`
    SELECT hex(content.ZDATA)
    FROM ZCDCHATMESSAGE AS message
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))}
    ORDER BY message.Z_PK;
  `);
  return (values ?? "")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((value) => decodeTextMessage(value))
    .filter((text) => text !== undefined);
}

/** True when a stored message of a room holds `text` as UTF-8 bytes. */
function roomDataContains(roomId, text) {
  const hex = Buffer.from(text, "utf8").toString("hex").toUpperCase();
  const count = sql(`
    SELECT count(*) FROM ZCDCHATMESSAGE AS message
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))}
    AND instr(hex(content.ZDATA), '${hex}') > 0;
  `);
  return Number(count ?? 0) > 0;
}

/**
 * True when the message that the chat list shows as the preview of a room
 * holds `text` as UTF-8 bytes.
 */
function previewContains(roomId, text) {
  const hex = Buffer.from(text, "utf8").toString("hex").toUpperCase();
  const count = sql(`
    SELECT count(*) FROM ZCDCHAT AS chat
    JOIN ZCDCHATMESSAGE AS message ON message.Z_PK = chat.ZLASTDISPLAYMESSAGE
    JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifierOf(roomId))}
    AND instr(hex(content.ZDATA), '${hex}') > 0;
  `);
  return Number(count ?? 0) > 0;
}

/** One query against the app's user-data store; undefined when it has none. */
function sql(query) {
  const database = userDataDatabase(appGroup);
  if (!existsSync(database)) return undefined;
  return captureOptional("sqlite3", [database, query]) || undefined;
}

function sqlString(value) {
  return `'${value.replaceAll("'", "''")}'`;
}
