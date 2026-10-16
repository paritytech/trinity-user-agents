// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Shared helpers of the two-emulator harness of a wasm chat product on
// Android (`launch-android-verbosity.mjs` and `android-verbosity.mjs`): the
// emulators of the users, the files that the launcher installs in the app,
// the state file of a run, and the reads and actions on one emulator.

import { randomBytes } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { parseLogcatLine, parseUiNodes } from "./android-chat-v2-scenarios.mjs";
import {
  amStart,
  chatIdHex,
  inputKeyCombination,
  inputKeyEvent,
  inputTap,
  pushFile,
  runAsSqliteJson,
  screencap,
  sendBroadcast,
  shell,
  uiDump,
} from "./android-emulator.mjs";
import { delay, waitFor } from "./process.mjs";

/** The users of a run, one for each emulator. */
export const USERS = ["a", "b"];

/** The isolated AVD and the console port of the emulator of each user. */
export const EMULATORS = Object.freeze({
  a: { avd: "verbosity-android-a", port: 5740 },
  b: { avd: "verbosity-android-b", port: 5742 },
});

export const E2E_ACTION = "io.paritytech.polkadotapp.debug.E2E";
export const E2E_TAG = "truapi.e2e";
export const RECEIVER = "io.paritytech.polkadotapp.app.debug.e2e.E2EHookReceiver";
export const ACTIVITY = "io.paritytech.polkadotapp.app.root.presentation.root.RootActivity";
export const DATABASE = "databases/app_v2.db";

/** How long a step of the room screen has to show. */
export const SCREEN_TIMEOUT_MS = 30_000;

const KEY = { back: 4, home: 3, del: 67, paste: 279, ctrlLeft: 113, a: 29 };

/**
 * The files of a worker that the launcher installs by hand in the app
 * (`files/Products/<host>/ChatExtension/`), and the `file:` worker URL that
 * names the entry file. The app runs a wasm entry file in the core, and reads
 * only the `keepAlive` field of the manifest beside it.
 */
export function handInstalledWorker(packageName, productHost) {
  const directory = `files/Products/${productHost}/ChatExtension`;
  return {
    directory,
    entry: `${directory}/index.wasm`,
    manifest: `${directory}/manifest.json`,
    url: `file:///data/user/0/${packageName}/${directory}/index.wasm`,
  };
}

/** The room id of a product room, from the hex of its chat id, or undefined for another chat. */
export function productRoomId(chatHex, productHost) {
  const prefix = `ChatExtension:ProductBot_${productHost}:`;
  const id = Buffer.from(chatHex, "hex").toString("utf8");
  return id.startsWith(prefix) ? id.slice(prefix.length) : undefined;
}

/** The wasm log lines of the worker in a logcat text (`truapi.core` with marker `wasm-worker`). */
export function workerLogLines(text) {
  return (text ?? "")
    .split(/\r?\n/)
    .map(parseLogcatLine)
    .filter((line) => line?.tag === "truapi.core" && line.message.startsWith("wasm-worker: "))
    .map((line) => `${line.time} ${line.message.slice("wasm-worker: ".length)}`);
}

/** Write the state of a run, which `android-verbosity.mjs` reads. */
export function writeState(path, state) {
  writeFileSync(path, `${JSON.stringify(state, null, 2)}\n`);
}

export function readState(path) {
  if (!existsSync(path)) {
    throw new Error(`no run state at ${path}; start make android-verbosity-run first`);
  }
  return JSON.parse(readFileSync(path, "utf8"));
}

/**
 * Copy `bytes` into the private files of the app at `path` (relative to its
 * data folder), through `run-as`. The bytes go through a file on the device,
 * never through a command line, so a mnemonic is not in the process list.
 */
export function writeAppFile(serial, packageName, path, bytes, { mode } = {}) {
  const local = mkdtempSync(join(tmpdir(), "android-verbosity-"));
  const source = join(local, "file");
  const staged = `/data/local/tmp/verbosity-${randomBytes(6).toString("hex")}`;
  try {
    writeFileSync(source, bytes, { mode: 0o600 });
    chmodSync(source, 0o600);
    pushFile(serial, source, staged);
    const directory = path.split("/").slice(0, -1).join("/");
    const chmod = mode ? ` && chmod ${mode} ${path}` : "";
    const result = shell(serial, [
      "sh",
      "-c",
      `cat ${staged} | run-as ${packageName} sh -c 'mkdir -p ${directory} && cat > ${path}${chmod}'`,
    ]);
    if (result.status !== 0) {
      throw new Error(`could not write ${path} in ${packageName}: ${result.stderr || result.stdout}`);
    }
  } finally {
    shell(serial, ["rm", "-f", staged]);
    rmSync(local, { recursive: true, force: true });
  }
}

/**
 * One emulator of a run: its app, the logcat of the app (`<out>/<user>.log`),
 * and the room screens of the product. Every read of the screen takes the
 * lock of the emulator, so the launcher, which answers permission prompts,
 * and a driver do not dump the screen at the same time.
 */
export class EmulatorUser {
  constructor({ name, serial, packageName, productHost, appLog, lock }) {
    Object.assign(this, { name, serial, packageName, productHost, appLog, lock });
    this.extensionId = `ProductBot_${productHost}`;
  }

  /** The logcat of the device since the launch. */
  console() {
    return existsSync(this.appLog) ? readFileSync(this.appLog, "utf8") : "";
  }

  /** The log lines of the wasm worker. */
  workerLog() {
    return workerLogLines(this.console());
  }

  /** The rooms of the product in the app database, with their names. */
  rooms() {
    return this.sql(`SELECT hex(id) AS id, COALESCE(name, '') AS name FROM chat_rooms ORDER BY rowid`)
      .map((row) => ({ room: productRoomId(row.id, this.productHost), name: row.name }))
      .filter((row) => row.room !== undefined);
  }

  /** The messages of a room, oldest first: text, type and status of each. */
  messages(room) {
    return this.sql(
      `SELECT id, type, status, origintype AS origin, COALESCE(searchableContent, '') AS text FROM chat_messages WHERE chatId = X'${this.chatHex(room)}' ORDER BY sortOrder, timestamp`,
    );
  }

  /** Wait until a room holds a message whose text contains `text` or matches it. */
  async waitForMessage(room, text, timeoutMs = 120_000) {
    const matches = (message) =>
      text instanceof RegExp ? text.test(message.text) : message.text.includes(text);
    return waitFor(() => this.messages(room).find(matches), {
      timeoutMs,
      intervalMs: 1_000,
      message: () =>
        `${this.name}: no message with ${text} in ${room} (has ${JSON.stringify(this.messages(room).map((m) => m.text))})`,
    });
  }

  /** Wait until the product has a room whose id or name matches `pattern`. */
  async waitForRoom(pattern, timeoutMs = 120_000) {
    return waitFor(
      () => this.rooms().find((room) => pattern.test(room.room) || pattern.test(room.name)),
      {
        timeoutMs,
        intervalMs: 1_000,
        message: () => `${this.name}: no room matches ${pattern} (has ${JSON.stringify(this.rooms())})`,
      },
    );
  }

  /** Open the screen of a room through the chat deeplink, as a tap on its row does. */
  async open(room) {
    const screen = await this.screen();
    if (this.currentRoom === room && findTag(screen, "chat_message_input")) return screen;
    // The system turns a keyboard on again when none is on; a keyboard covers the feed in the screenshots.
    for (const ime of KEYBOARDS) shell(this.serial, ["ime", "disable", ime]);
    amStart(this.serial, [
      "-a",
      "android.intent.action.VIEW",
      "-d",
      `polkadotapp://chat?chatId=${this.chatHex(room)}`,
      this.packageName,
    ]);
    this.currentRoom = room;
    const name = this.rooms().find((entry) => entry.room === room)?.name;
    return this.waitScreen(
      (nodes) => findTag(nodes, "chat_message_input") && (!name || findText(nodes, name)) && nodes,
      `the ${room} room screen`,
    );
  }

  /** Type `text` into a room and send it, as the user does on its screen. */
  async send(room, text) {
    await this.open(room);
    let typed;
    for (let attempt = 1; attempt <= 3 && !typed; attempt += 1) {
      const screen = await this.waitScreen((nodes) => findTag(nodes, "chat_message_input") && nodes, "the message input");
      const input = findTag(screen, "chat_message_input");
      inputTap(this.serial, input.center.x, input.center.y);
      inputKeyCombination(this.serial, KEY.ctrlLeft, KEY.a);
      inputKeyEvent(this.serial, KEY.del);
      // The app can lose the key events of `input text`, so the text goes through the clipboard.
      await this.hookAndWait([["--es", "clipboard", text]], "clipboard set");
      inputKeyEvent(this.serial, KEY.paste);
      typed = await this.waitScreen(
        (nodes) => findTag(nodes, "chat_message_input")?.text === text && findTag(nodes, "chat_send_button") && nodes,
        `"${text}" in the input`,
        5_000,
      ).catch(() => undefined);
    }
    if (!typed) throw new Error(`${this.name}: could not type "${text}" into the input`);
    const send = findTag(typed, "chat_send_button");
    inputTap(this.serial, send.center.x, send.center.y);
    await this.waitScreen((nodes) => findTag(nodes, "chat_message_input")?.text !== text && nodes, "the input to clear");
    if (/mInputShown=true/.test(shell(this.serial, ["dumpsys", "input_method"]).stdout)) {
      inputKeyEvent(this.serial, KEY.back);
    }
  }

  /**
   * Press the button `label` of a message in a room. With `near`, the
   * button of the newest message whose text contains `near`; else the lowest
   * button with the label on the screen, which is the newest.
   */
  async press(room, label, near) {
    await this.open(room);
    const button = await this.waitScreen((nodes) => {
      const buttons = nodes.filter((node) => node.text.toLowerCase() === label.toLowerCase());
      if (buttons.length === 0) return undefined;
      if (!near) return buttons.sort((a, b) => b.center.y - a.center.y)[0];
      const anchor = nodes
        .filter((node) => node.text.includes(near))
        .sort((a, b) => b.center.y - a.center.y)[0];
      if (!anchor) return undefined;
      return buttons
        .filter((node) => node.center.y > anchor.center.y)
        .sort((a, b) => a.center.y - b.center.y)[0];
    }, `the button "${label}"`);
    inputTap(this.serial, button.center.x, button.center.y);
    await delay(500);
  }

  /** The texts on the screen, top to bottom. */
  async texts() {
    return (await this.screen()).map((node) => node.text || node.description).filter(Boolean);
  }

  /** Send the app to the background (Home), or bring it back. */
  background() {
    this.currentRoom = undefined;
    return inputKeyEvent(this.serial, KEY.home);
  }

  foreground() {
    return amStart(this.serial, ["-n", `${this.packageName}/${ACTIVITY}`]);
  }

  screenshot(path) {
    return screencap(this.serial, path);
  }

  /** Send the debug hook of the app `extras`, and wait for the log line `ack`. */
  async hookAndWait(extras, ack, timeoutMs = SCREEN_TIMEOUT_MS) {
    const before = this.console().length;
    const result = sendBroadcast(this.serial, {
      action: E2E_ACTION,
      component: `${this.packageName}/${RECEIVER}`,
      extras,
    });
    if (result.error) throw new Error(`${this.name}: the debug broadcast was rejected: ${result.error}`);
    return waitFor(
      () => {
        const lines = this.console().slice(before).split(/\r?\n/).map(parseLogcatLine);
        const failure = lines.find((line) => line?.tag === E2E_TAG && line.message.startsWith("error "));
        if (failure) throw new Error(`${this.name}: ${failure.message}`);
        return lines.find((line) => line?.tag === E2E_TAG && line.message.startsWith(ack))?.message;
      },
      { timeoutMs, intervalMs: 300, message: `${this.name}: no "${ack}" from the debug hook` },
    );
  }

  /** The nodes of the screen. Closes the prompts that `answer` names first, when given. */
  async screen() {
    return this.withLock(() => parseUiNodes(uiDump(this.serial)));
  }

  async waitScreen(predicate, what, timeoutMs = SCREEN_TIMEOUT_MS) {
    let last = [];
    return waitFor(
      async () => {
        last = await this.screen();
        if (await this.answerPrompt(last)) return undefined;
        // The first screens show a hint over the network rings; a tap closes it.
        const hint = findText(last, "Network status");
        if (hint) {
          inputTap(this.serial, hint.center.x, hint.center.y);
          await delay(500);
          return undefined;
        }
        return predicate(last);
      },
      {
        timeoutMs,
        intervalMs: 500,
        message: () => `${this.name}: no ${what} on screen; it shows ${summary(last)}`,
      },
    );
  }

  /**
   * Answer a permission sheet of the product with `answer` ("Allow always" by
   * default), as the user would. Gives the label that it tapped, or undefined.
   */
  async answerPrompt(nodes, answer = this.promptAnswer ?? "Allow always") {
    if (answer === "ask") return undefined;
    const sheet = nodes.some(
      (node) => node.text === `Requested by ${this.productHost}` || PROMPT_TEXT.test(node.text),
    );
    if (!sheet) return undefined;
    // An identity sheet has Approve and Reject; a permission sheet has the answers of a permission.
    const labels = answer === "Allow always" ? [answer, "Approve", "Allow"] : [answer];
    const button = labels.map((label) => findText(nodes, label)).find(Boolean);
    if (!button) return undefined;
    inputTap(this.serial, button.center.x, button.center.y);
    await delay(800);
    return button.text;
  }

  async withLock(action) {
    if (!this.lock) return action();
    const deadline = Date.now() + 120_000;
    while (true) {
      try {
        mkdirSync(this.lock);
        break;
      } catch (error) {
        if (error.code !== "EEXIST") throw error;
        if (Date.now() > deadline) rmSync(this.lock, { recursive: true, force: true });
        await delay(200);
      }
    }
    try {
      return await action();
    } finally {
      rmSync(this.lock, { recursive: true, force: true });
    }
  }

  chatHex(room) {
    return chatIdHex(this.extensionId, room);
  }

  sql(query) {
    try {
      return runAsSqliteJson(this.serial, this.packageName, DATABASE, query);
    } catch {
      return [];
    }
  }
}

const KEYBOARDS = [
  "com.google.android.inputmethod.latin/com.android.inputmethod.latin.LatinIME",
  "com.google.android.tts/com.google.android.apps.speech.tts.googletts.settings.asr.voiceime.VoiceInputMethodService",
];

/** The texts of a permission sheet of a product. */
const PROMPT_TEXT = / would like to | wants to |Allow .* to /;

export function findTag(nodes, tag) {
  return nodes.find((node) => node.tag === tag);
}

export function findText(nodes, text) {
  return nodes.find((node) => (text instanceof RegExp ? text.test(node.text) : node.text === text));
}

function summary(nodes) {
  const texts = nodes.map((node) => node.text || node.description).filter(Boolean);
  return JSON.stringify(texts.slice(0, 30).join(" | ").slice(0, 600));
}
