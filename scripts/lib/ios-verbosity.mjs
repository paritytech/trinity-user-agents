// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Shared helpers of the two-simulator harness of a wasm chat product
// (`launch-ios-verbosity.mjs` and `ios-verbosity.mjs`): the live identities of
// the users, the state file of a run, and the reads and actions on one
// simulator.

import { spawn } from "node:child_process";
import { randomBytes } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  renameSync,
  writeFileSync,
} from "node:fs";
import { resolve } from "node:path";

import { decodeTextMessage } from "./chat-diagnosis-report.mjs";
import { parseUiMarkers } from "./ios-chat-v2-scenarios.mjs";
import { userDataDatabase } from "./ios-simulator.mjs";
import { captureOptional, delay, waitFor } from "./process.mjs";

/**
 * The text of a stored message (`ZCDMESSAGECONTENT.ZDATA`, the SCALE
 * `ChatMessageContent`, in hex): the text of a `Text` message, or the text of
 * an `Actions` message (a message with buttons, such as a contact request)
 * followed by its action ids in brackets. Other kinds give undefined.
 */
export function decodeMessageText(hex) {
  const encoded = Buffer.from(hex, "hex");
  if (encoded[0] === 0) return decodeTextMessage(hex);
  // `Actions(ChatActions { text: Option<String>, actions, layout })`.
  if (encoded[0] !== 2) return undefined;
  let offset = 1;
  let text = "";
  if (encoded[offset] === 1) {
    [text, offset] = string(encoded, offset + 1);
  } else {
    offset += 1;
  }
  // Each `ChatAction` is `{ action_id: String, title: String }`.
  const count = compact(encoded, offset);
  offset += count.bytes;
  const ids = [];
  for (let index = 0; index < count.value; index += 1) {
    const [id, afterId] = string(encoded, offset);
    const [, afterTitle] = string(encoded, afterId);
    ids.push(id);
    offset = afterTitle;
  }
  return ids.length > 0 ? `${text} [${ids.join(", ")}]` : text;
}

/** The SCALE `String` at `offset`, and the offset after it. */
function string(encoded, offset) {
  const length = compact(encoded, offset);
  const start = offset + length.bytes;
  return [
    encoded.subarray(start, start + length.value).toString("utf8"),
    start + length.value,
  ];
}

function compact(encoded, offset) {
  const first = encoded[offset];
  const mode = first & 0b11;
  if (mode === 0) return { value: first >> 2, bytes: 1 };
  if (mode === 1) return { value: encoded.readUInt16LE(offset) >> 2, bytes: 2 };
  if (mode === 2)
    return { value: encoded.readUInt32LE(offset) >>> 2, bytes: 4 };
  throw new Error(
    "a SCALE compact value this large is not expected in a message",
  );
}

/** The users of a run, one for each simulator. */
export const USERS = ["a", "b"];

/** How long a command has to reach the app. */
export const COMMAND_TIMEOUT_MS = 30_000;

/**
 * The Worker executable manifest that the launcher installs beside the module
 * (`ChatExtension/manifest.json`). The app reads its `keepAlive` field: a
 * `foreground` worker runs while the app is in the foreground, with no room
 * screen open and no hold of the harness, as for a user (VERBOSITY-WASM-PRODUCT
 * I5).
 */
export const WORKER_MANIFEST = Object.freeze({
  $v: 1,
  kind: "worker",
  appVersion: [0, 1, 0],
  entrypoint: "index.wasm",
  includes: { chat: true, pocket: false },
  keepAlive: "foreground",
});

/**
 * The environment of the app of one user. The command feed is on, and the
 * harness holds the worker only when `holdWorker` is true: else the worker
 * runs as the foreground worker that its manifest declares. With `wakeRelay`
 * (a URL of `truapi-host wake-relay`) the app registers the wake topics of
 * its products there and takes wake pushes from the relay's `simulator`
 * sink, and with `osNotifications` the notifications of a
 * product go to the OS, which the launcher gave the permission
 * (VERBOSITY-WASM-PRODUCT A1).
 */
export function launchEnvironment({
  productHost,
  productName,
  holdWorker,
  wakeRelay,
  osNotifications,
}) {
  return {
    ...(wakeRelay
      ? {
          SIMCTL_CHILD_TRUAPI_IOS_E2E_WAKE_RELAY_URL: wakeRelay,
          SIMCTL_CHILD_TRUAPI_IOS_E2E_WAKE_PUSH_FEED: "1",
        }
      : {}),
    ...(osNotifications
      ? { SIMCTL_CHILD_TRUAPI_IOS_E2E_OS_NOTIFICATIONS: "1" }
      : {}),
    SIMCTL_CHILD_RUST_BACKTRACE: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_RESTORE_WALLET: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST: productHost,
    SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME: productName,
    SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_COMMANDS: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_HOLD_WORKER: holdWorker ? "1" : "0",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_RUNTIME_MARKERS: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_NO_PUSH_PROMPT: "1",
    SIMCTL_CHILD_TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS: "1",
  };
}

/**
 * The live account of one user: a mnemonic and a registered lite username,
 * which `truapi-host signing-host` makes on the network the app uses. The
 * CLI registers the username through the identity backend and waits for the
 * ring membership on People, as the claim screen of the app does. Nothing is
 * faked: the app restores the wallet from the mnemonic and finds the username
 * on chain itself. The statement-store allowance of the product is allocated
 * by the core of the app on the first `createProofAuthorized`.
 *
 * An account made by an earlier run is used again. The mnemonic stays in the
 * CLI's own files under `basePath` and is never printed.
 */
export async function provisionIdentity({
  basePath,
  session,
  network,
  productHost,
  timeoutMs = 15 * 60_000,
}) {
  const existing = readIdentity(basePath, network);
  if (existing) return existing;

  mkdirSync(basePath, { recursive: true });
  const log = resolve(basePath, "provision.log");
  let output = "";
  const child = spawn(
    "truapi-host",
    [
      "signing-host",
      "--serve",
      "--base-path",
      basePath,
      "--session",
      session,
      "--network",
      network,
      "--product-id",
      productHost,
      "--auto-accept",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  for (const stream of [child.stdout, child.stderr]) {
    stream.on("data", (chunk) => {
      output += chunk.toString();
      writeFileSync(log, output);
    });
  }
  let exited = false;
  child.on("exit", () => {
    exited = true;
  });
  try {
    await waitFor(
      () => {
        if (/Signing host ready/.test(output)) return true;
        if (exited || /^Error:/m.test(output)) {
          throw new Error(
            `truapi-host could not make the account of ${session}; see ${log}`,
          );
        }
        return undefined;
      },
      {
        timeoutMs,
        intervalMs: 1_000,
        message: `truapi-host did not register ${session} in time; see ${log}`,
      },
    );
  } finally {
    child.kill("SIGTERM");
  }
  const identity = readIdentity(basePath, network);
  if (!identity) {
    throw new Error(`truapi-host stored no account under ${basePath}`);
  }
  return identity;
}

/**
 * The attested account that `truapi-host` stored under `basePath` for
 * `network`, or undefined. Each signing session keeps its account in
 * `v2/<network>/<username>_signing_host/accounts.json`.
 */
export function readIdentity(basePath, network) {
  const sessions = resolve(basePath, "v2", network);
  if (!existsSync(sessions)) return undefined;
  for (const session of readdirSync(sessions).sort().reverse()) {
    const file = resolve(sessions, session, "accounts.json");
    if (!session.endsWith("_signing_host") || !existsSync(file)) continue;
    const account = JSON.parse(readFileSync(file, "utf8")).accounts?.find(
      (candidate) =>
        candidate.attested &&
        candidate.network === network &&
        candidate.lite_username &&
        candidate.mnemonic,
    );
    if (account) {
      return { username: account.lite_username, mnemonic: account.mnemonic };
    }
  }
  return undefined;
}

/** Write the state of a run, which `ios-verbosity.mjs` reads. */
export function writeState(path, state) {
  writeFileSync(path, `${JSON.stringify(state, null, 2)}\n`);
}

export function readState(path) {
  if (!existsSync(path)) {
    throw new Error(
      `no run state at ${path}; start make ios-verbosity-run first`,
    );
  }
  return JSON.parse(readFileSync(path, "utf8"));
}

/**
 * One simulator of a run: its app containers, its console log, and the
 * command feed of the app (`TrUAPIE2ECommandFeed`).
 */
export class SimulatorUser {
  constructor({ name, udid, appData, appGroup, appLog, productHost }) {
    Object.assign(this, { name, udid, appData, appGroup, appLog, productHost });
    this.commandDir = resolve(appData, "tmp/truapi-e2e/commands");
  }

  /** The console of the app since the launch. */
  console() {
    return existsSync(this.appLog) ? readFileSync(this.appLog, "utf8") : "";
  }

  /** The `[e2e:ui]` markers of the app, optionally of one key. */
  markers(key) {
    const markers = parseUiMarkers(this.console());
    return key ? markers.filter((marker) => marker.key === key) : markers;
  }

  /** The log lines of the wasm worker. */
  workerLog() {
    return this.console()
      .split(/\r?\n/)
      .filter((line) => line.includes("[truapi:wasm-worker]"));
  }

  /** The rooms of the product in Core Data, with their names. */
  rooms() {
    const prefix = `1:${this.productHost}:`;
    const rows = this.sql(`
      SELECT chat.ZIDENTIFIER, coalesce(metadata.ZNAME, '')
      FROM ZCDCHAT AS chat
      LEFT JOIN ZCDCHATROOMMETADATA AS metadata ON metadata.ZCHAT = chat.Z_PK
      WHERE chat.ZIDENTIFIER LIKE ${sqlString(`${prefix}%`)}
      ORDER BY chat.Z_PK;
    `);
    return (rows ?? "")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => {
        const [identifier, name] = line.split("|");
        return { room: identifier.slice(prefix.length), name };
      });
  }

  /** The text messages of a room, oldest first. */
  messages(room) {
    const values = this.sql(`
      SELECT hex(content.ZDATA)
      FROM ZCDCHATMESSAGE AS message
      JOIN ZCDCHAT AS chat ON chat.Z_PK = message.ZCHAT
      JOIN ZCDMESSAGECONTENT AS content ON content.Z_PK = message.ZCONTENT
      WHERE chat.ZIDENTIFIER = ${sqlString(this.chatIdentifier(room))}
      ORDER BY message.Z_PK;
    `);
    return (values ?? "")
      .split(/\r?\n/)
      .filter(Boolean)
      .map((value) => decodeMessageText(value))
      .filter((text) => text !== undefined);
  }

  /** Wait until a room holds a message that contains `text`. */
  async waitForMessage(room, text, timeoutMs = 120_000) {
    return waitFor(
      () => this.messages(room).find((message) => message.includes(text)),
      {
        timeoutMs,
        intervalMs: 500,
        message: () =>
          `${this.name}: no message with "${text}" in ${room} (has ${JSON.stringify(this.messages(room))})`,
      },
    );
  }

  /** Wait until the product has a room whose id or name matches `pattern`. */
  async waitForRoom(pattern, timeoutMs = 120_000) {
    return waitFor(
      () =>
        this.rooms().find(
          (room) => pattern.test(room.room) || pattern.test(room.name),
        ),
      {
        timeoutMs,
        intervalMs: 500,
        message: () =>
          `${this.name}: no room matches ${pattern} (has ${JSON.stringify(this.rooms())})`,
      },
    );
  }

  /** Open the screen of a room, through the link pipeline of the app. */
  async open(room) {
    const before = this.roomMarkers(room).length;
    await this.queue({ room, host: "open" }, `open ${room}`);
    // A new screen writes its header and footer markers. A screen that was
    // open already writes none, so the wait ends quietly.
    await waitFor(() => this.roomMarkers(room).length > before, {
      timeoutMs: 5_000,
      message: "",
    }).catch(() => {});
    await delay(500);
  }

  roomMarkers(room) {
    return this.markers().filter(
      (marker) => marker.room === room && marker.key !== "action",
    );
  }

  /** Type `text` into a room and send it, as the user does on its screen. */
  async send(room, text) {
    await this.open(room);
    await this.queue({ room, host: "send", argument: text }, `send in ${room}`);
  }

  /** Press the action `actionId` of a message (`last` is the newest message of the product). */
  async press(room, actionId, message = "last") {
    await this.open(room);
    await this.queue(
      { room, host: "button", target: message, argument: actionId },
      `press ${actionId} in ${room}`,
    );
  }

  /** Any other action of the room screen (`TrUAPIE2EHostAction`). */
  async action(room, host, argument, target) {
    await this.queue({ room, host, argument, target }, `${host} in ${room}`);
  }

  /** Give one command file to the feed of the app, and wait until the app takes it. */
  async queue(payload, label) {
    mkdirSync(this.commandDir, { recursive: true });
    // The feed sends the files in name order, also across helper processes.
    const name = `${Date.now()}-${randomBytes(3).toString("hex")}`;
    const pending = resolve(this.commandDir, `${name}.tmp`);
    writeFileSync(pending, JSON.stringify(payload));
    renameSync(pending, resolve(this.commandDir, `${name}.json`));
    const outcome = await waitFor(
      () => {
        if (existsSync(resolve(this.commandDir, `${name}.done`))) return "done";
        const failed = resolve(this.commandDir, `${name}.failed`);
        return existsSync(failed)
          ? `failed: ${readFileSync(failed, "utf8")}`
          : undefined;
      },
      {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: `${this.name}: the app did not take ${label}`,
      },
    );
    if (outcome !== "done")
      throw new Error(`${this.name}: ${label} ${outcome}`);
    await delay(300);
  }

  screenshot(path) {
    return captureOptional("xcrun", [
      "simctl",
      "io",
      this.udid,
      "screenshot",
      path,
    ]);
  }

  chatIdentifier(room) {
    return `1:${this.productHost}:${room}`;
  }

  sql(query) {
    const database = userDataDatabase(this.appGroup);
    if (!existsSync(database)) return undefined;
    return captureOptional("sqlite3", [database, query]) || undefined;
  }
}

function sqlString(value) {
  return `'${value.replaceAll("'", "''")}'`;
}
