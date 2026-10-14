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
// bot, its `host` room open, and a command feed. It then sends the commands of
// each scenario that the app of today serves, checks Core Data and the app's
// console, and takes a screenshot. Scenarios that need app work that is not
// built yet get a `skip` row that names the phase that adds it. The report
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
  IOS_CHAT_V2_SCENARIOS,
  appStartFailure,
  blockedRows,
  parseEventLog,
  refusalReason,
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
const commandDir = resolve(appData, "tmp/truapi-e2e/commands");
rmSync(commandDir, { recursive: true, force: true });
mkdirSync(commandDir, { recursive: true });

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
      SIMCTL_CHILD_TRUAPI_IOS_E2E_OPEN_CHAT: "1",
      SIMCTL_CHILD_TRUAPI_IOS_E2E_RUNTIME_MARKERS: "1",
    },
    stdio: ["ignore", "pipe", "pipe"],
  },
);
let consoleText = "";
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
    rows.push(...blockedRows(`${started} (\`start.png\`)`));
    failure = started;
  } else {
    for (const scenario of IOS_CHAT_V2_SCENARIOS) {
      rows.push(await runScenario(scenario));
    }
  }
} catch (error) {
  failure = error instanceof Error ? error.message : String(error);
  for (const scenario of IOS_CHAT_V2_SCENARIOS) {
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
  IOS_CHAT_V2_SCENARIOS.filter((scenario) => scenario.drive),
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
      return roomName() ? true : undefined;
    },
    {
      timeoutMs: START_TIMEOUT_MS,
      message: () =>
        `Timed out waiting for the product to create ${chatIdentifier}; see ${relative(repoRoot, appLogPath)}`,
    },
  );
}

async function runScenario(scenario) {
  if (scenario.drive) return driveScenario(scenario);
  if (scenario.observe) return observeScenario(scenario);
  return skipRow(scenario);
}

async function driveScenario(scenario) {
  const started = Date.now();
  try {
    for (const command of scenario.drive.commands) {
      await sendCommand(command);
    }
    const evidence = await waitForExpectation(scenario.drive.expect);
    const screenshot = await takeScreenshot(scenario);
    for (const command of scenario.drive.after ?? []) {
      await sendCommand(command);
    }
    const note = scenario.note ? `; ${scenario.note}` : "";
    return row(
      scenario,
      "pass",
      Date.now() - started,
      `\`${screenshot}\`; ${evidence}${note}`,
    );
  } catch (error) {
    const screenshot = await takeScreenshot(scenario).catch(() => undefined);
    const shot = screenshot ? `\`${screenshot}\`; ` : "";
    return row(
      scenario,
      "fail",
      Date.now() - started,
      `${shot}${error.message}`,
    );
  }
}

async function observeScenario(scenario) {
  const { command, refusal } = scenario.observe;
  try {
    await sendCommand(command);
    const reason = await waitFor(() => refusalReason(consoleText, refusal), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `no refusal of ${refusal} in the console`,
    });
    return skipRow(
      scenario,
      `\`${command}\` reached the product; the app answered ${reason}`,
    );
  } catch (error) {
    return skipRow(scenario, `\`${command}\`: ${error.message}`);
  }
}

let commandCounter = 0;

/** Queue one command for the app's command feed, and wait until it is sent. */
async function sendCommand(text) {
  commandCounter += 1;
  const name = String(commandCounter).padStart(4, "0");
  const pending = resolve(commandDir, `${name}.tmp`);
  writeFileSync(pending, JSON.stringify({ room, text }));
  cpSync(pending, resolve(commandDir, `${name}.json`));
  rmSync(pending);
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
      message: `the app did not send \`${text}\``,
    },
  );
  if (outcome !== "done") throw new Error(`\`${text}\` ${outcome}`);
  // The product receives the command as a `MessagePosted` event.
  await waitFor(
    () =>
      parseEventLog(consoleText).some(
        (entry) =>
          entry.via === "action_subscribe" &&
          entry.kind === "MessagePosted" &&
          JSON.stringify(entry.data).includes(
            JSON.stringify(text).slice(1, -1),
          ),
      ),
    {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `the product did not log \`${text}\` as MessagePosted`,
    },
  );
}

async function waitForExpectation(expect) {
  if (expect.room) {
    const name = await waitFor(() => roomName(), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `no room ${chatIdentifier}`,
    });
    if (name !== expect.room) {
      throw new Error(
        `room name is ${JSON.stringify(name)}, not ${JSON.stringify(expect.room)}`,
      );
    }
    return `room \`${chatIdentifier}\` named "${name}"`;
  }
  if (expect.text) {
    await waitFor(() => messageTexts().some((text) => text === expect.text), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `no message "${expect.text}" in ${chatIdentifier}`,
    });
    return `message "${expect.text}" stored`;
  }
  if (expect.log) {
    await waitFor(() => consoleText.includes(expect.log), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `no "${expect.log}" in the console`,
    });
    const footer = sql(`
      SELECT chat.ZFOOTER FROM ZCDCHAT AS chat
      WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifier)};
    `);
    return `console "${expect.log}"; stored footer ${JSON.stringify(footer ?? null)}`;
  }
  throw new Error("the scenario names no expectation");
}

async function takeScreenshot(scenario) {
  // The bubble or the footer change needs a moment to reach the screen.
  await delay(1_500);
  const name = `${scenario.id.toLowerCase().replace(/^s(\d)$/, "s0$1")}-${scenario.mode}.png`;
  run(
    "xcrun",
    ["simctl", "io", device.udid, "screenshot", resolve(outDir, name)],
    {
      stdio: "ignore",
    },
  );
  return name;
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

function messageTexts() {
  const values = sql(`
    SELECT hex(content.ZDATA)
    FROM ZCDCHATMESSAGE AS message
    JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
    JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
    WHERE chat.ZIDENTIFIER = ${sqlString(chatIdentifier)}
    ORDER BY message.Z_PK;
  `);
  return (values ?? "")
    .split(/\r?\n/)
    .filter(Boolean)
    .map((value) => decodeTextMessage(value))
    .filter((text) => text !== undefined);
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
