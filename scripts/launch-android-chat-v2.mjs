#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Run the Chat Extension v2 sample product (`playground/chat-v2`) in the
// Android emulator app, and write one diagnosis row for each scenario of the
// test plan that names the Android layer. This is the twin of
// `launch-ios-chat-v2.mjs`.
//
// The launcher builds and serves the product, installs the debug APK, and
// uses the debug-only E2E hooks to seed an identity and to install the
// product as a chat bot. It then drives the app as a user does: it opens the
// `host` and `source` rooms through the chat deeplink, types the commands of
// the sample product into the room input, taps buttons and menus, and
// scrolls. Each row is proven from the event log of the worker (its console
// lines in logcat), the app database (`run-as sqlite3`), the screen
// (`uiautomator dump`) and a screenshot. Scenarios that need app work that
// is not built yet get a `skip` row that names the phase that adds it.
//
// The report goes to `playground/test-results/android-chat-v2/`, with the
// screenshots, the app's logcat (`app.log`) and the event log beside it.
// The launcher fails when a driven scenario fails or has no row.

import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { relative, resolve } from "node:path";
import {
  DEFAULT_AVD,
  amStart,
  apkPackageName,
  chatIdHex,
  clearAppData,
  clearLogcat,
  ensureEmulator,
  forwardAbstractSocket,
  grantPermission,
  inputKeyCombination,
  inputKeyEvent,
  inputLongPress,
  inputSwipe,
  inputTap,
  installApk,
  isDeviceOnline,
  killEmulator,
  onAdbServerRestart,
  pushFile,
  removeForward,
  removeReversePort,
  reversePort,
  rootShell,
  runAsSqliteJson,
  screencap,
  sendBroadcast,
  shell,
  streamLogcat,
  uiDump,
  waitForProcess,
} from "./lib/android-emulator.mjs";
import {
  ANDROID_CHAT_FEATURE_ROWS,
  ANDROID_CHAT_V2_PERF,
  ANDROID_CHAT_V2_SCENARIOS,
  blockedRows,
  byScenarioNumber,
  capabilityVerdict,
  consoleMessages,
  parseEventLog,
  parseLogcatLine,
  parseUiNodes,
  row,
  skipRow,
} from "./lib/android-chat-v2-scenarios.mjs";
import { renderScenarioReport, scenarioFailures } from "./lib/cli-chat-scenarios.mjs";
import { delay, runAsync, waitFor } from "./lib/process.mjs";
import { startProductServer } from "./lib/product-server.mjs";

const repoRoot = resolve(import.meta.dirname, "..");

const DEBUG_ACTION = "io.paritytech.polkadotapp.debug.E2E";
const E2E_TAG = "truapi.e2e";
const RECEIVER = "io.paritytech.polkadotapp.app.debug.e2e.E2EHookReceiver";
const ACTIVITY = "io.paritytech.polkadotapp.app.root.presentation.root.RootActivity";
const DATABASE = "databases/app_v2.db";
const IMES = [
  "com.google.android.inputmethod.latin/com.android.inputmethod.latin.LatinIME",
  "com.google.android.tts/com.google.android.apps.speech.tts.googletts.settings.asr.voiceime.VoiceInputMethodService",
];
const KEY = { back: 4, home: 3, del: 67, paste: 279, ctrlLeft: 113, a: 29 };

/** How long the app has to start the product and create its rooms. */
const START_TIMEOUT_MS = 180_000;
/** How long one command has to reach the product and take effect. */
const COMMAND_TIMEOUT_MS = 30_000;
/** The time-out of a product answer in the app, plus a margin. */
const ANSWER_TIMEOUT_MS = 50_000;

const apk = resolve(
  repoRoot,
  process.env.TRUAPI_ANDROID_E2E_APK ??
    "hosts/android/app/build/outputs/apk/vanilla/debug/app-vanilla-debug.apk",
);
const packageName =
  process.env.TRUAPI_ANDROID_E2E_PACKAGE ?? apkPackageName(apk) ?? "com.example.polkadot.debug";
const avd = process.env.TRUAPI_ANDROID_E2E_AVD ?? DEFAULT_AVD;
const requestedSerial = process.env.TRUAPI_ANDROID_E2E_DEVICE;
const username = process.env.TRUAPI_ANDROID_E2E_USERNAME ?? "truapi-e2e";
const productRoot = resolve(
  repoRoot,
  process.env.TRUAPI_ANDROID_E2E_CHAT_PRODUCT_DIR ?? "playground/chat-v2",
);
const productHost = process.env.TRUAPI_ANDROID_E2E_CHAT_PRODUCT_HOST ?? "truapi-chat-v2.dot";
const productName =
  process.env.TRUAPI_ANDROID_E2E_CHAT_PRODUCT_NAME ?? "Chat Extension v2 Sample";
const productUrl = process.env.TRUAPI_ANDROID_E2E_CHAT_PRODUCT_URL ?? "http://127.0.0.1:3100";
const outDir = resolve(
  repoRoot,
  process.env.TRUAPI_ANDROID_E2E_CHAT_V2_OUT ?? "playground/test-results/android-chat-v2",
);
/** A comma-separated list of scenario ids: run only those (for development). */
const onlyScenarios = (process.env.TRUAPI_ANDROID_E2E_CHAT_V2_ONLY ?? "")
  .split(",")
  .map((id) => id.trim())
  .filter(Boolean);
/** Also drive the benchmark rows (B9) after the scenarios. */
const runPerf =
  process.env.TRUAPI_ANDROID_E2E_PERF === "1" ||
  ANDROID_CHAT_V2_PERF.some((row) => onlyScenarios.includes(row.id));
const ROWS = runPerf ? [...ANDROID_CHAT_V2_SCENARIOS, ...ANDROID_CHAT_V2_PERF] : ANDROID_CHAT_V2_SCENARIOS;
/** Keep the installed app and its data (for development). */
const reuseApp = process.env.TRUAPI_ANDROID_E2E_CHAT_V2_REUSE === "1";
const keepEmulator = process.env.TRUAPI_ANDROID_E2E_KEEP_EMULATOR === "1";

const url = new URL(productUrl);
if (url.hostname !== "127.0.0.1") {
  throw new Error(`The Android app only allows cleartext to 127.0.0.1; got ${url.hostname}`);
}
if (!existsSync(apk)) {
  throw new Error(
    `Debug APK not found: ${apk}\nBuild it first: cd hosts/android && ./gradlew :app:assembleVanillaDebug`,
  );
}
if (!existsSync(resolve(productRoot, "package.json"))) {
  throw new Error(`Chat product source not found: ${productRoot}`);
}

const productPort = url.port || "80";
const workerUrl = `${productUrl.replace(/\/$/, "")}/worker/index.js`;
const worker = resolve(productRoot, "out/worker/index.js");
const extensionId = `ProductBot_${productHost}`;
const ROOMS = {
  host: { id: "host", name: "Sample: host store", chat: chatIdHex(extensionId, "host") },
  source: { id: "source", name: "Sample: product source", chat: chatIdHex(extensionId, "source") },
  // The first `/placeholder` makes this room (S37).
  "host-empty": { id: "host-empty", name: "Sample: empty", chat: chatIdHex(extensionId, "host-empty") },
};
/** The attachments of S36: an image under the inline limit, and a file over it. */
const SMALL_ATTACHMENT = "sample.png";
const LARGE_ATTACHMENT = "large.bin";

/** A file of 2 MiB, over the inline limit of 1 MiB, in the output folder. */
function largeAttachment() {
  const path = resolve(outDir, LARGE_ATTACHMENT);
  writeFileSync(path, Buffer.alloc(2 * 1024 * 1024, 0x5a));
  return path;
}
const receiverComponent = `${packageName}/${RECEIVER}`;
const launcherActivity = `${packageName}/${ACTIVITY}`;

let serial;
let bootedEmulator = false;
let productServer;
let logcat;
let currentRoom;
/** What the launcher saw the app do outside a scenario check, for the report. */
const observations = [];


// ------------------------------------------------------------- scenarios

async function runScenario(scenario) {
  const driver = DRIVERS[scenario.id];
  if (!scenario.drive || !driver) return skipRow(scenario);
  const started = Date.now();
  const evidence = [];
  const note = (text) => evidence.push(text);
  try {
    await driver({ scenario, note });
    return row(scenario, "pass", Date.now() - started, evidence.join("; "));
  } catch (error) {
    const shot = await screenshot(scenario, "fail").catch(() => undefined);
    const reason = error instanceof Error ? error.message : String(error);
    return row(
      scenario,
      "fail",
      Date.now() - started,
      [...(shot ? [`\`${shot}\``] : []), ...evidence, `FAILED: ${reason}`].join("; "),
    );
  }
}

/** The Chat features that the app lists to the product, as S49 reads them from logcat. */
let listedFeatures = [];

const DRIVERS = {
  async S2({ note }) {
    const declarations = Object.fromEntries(
      ["host", "source"].map((room) => [room, declarationRow(room)]),
    );
    for (const room of ["host", "source"]) {
      const stored = roomRow(room);
      expect(stored?.name === ROOMS[room].name, `room ${room} is named ${JSON.stringify(stored?.name)}`);
      expect(declarations[room], `room ${room} has no stored declaration`);
      expect(declarations[room].description, `room ${room} has no description`);
    }
    expect(declarations.source.cfg !== "", "the source room has no stored config");
    expect(declarations.source.p !== "", "the source room has no stored preview");
    // The product declares the host room with the default configuration, which the host stores as zero bytes.
    expect(/^0*$/.test(declarations.host.cfg) && declarations.host.p === "", `the host room stores config 0x${declarations.host.cfg} and preview 0x${declarations.host.p}`);
    await openRoom("host");
    const nodes = await waitScreen(
      (screen) => findText(screen, ROOMS.host.name) && findText(screen, declarations.host.description) && screen,
      "the host room header",
    );
    expect(nodes, "no header");
    note(`\`${await screenshot(scenarioById("S2"), "host")}\``);
    note(
      `chat_rooms: "${ROOMS.host.name}", "${ROOMS.source.name}"; chat_room_declarations: host description "${declarations.host.description}" with default config and no preview, source config 0x${declarations.source.cfg} and preview 0x${declarations.source.p}; header shows name and description; the declarations stayed after the app restart, whose worker declared the rooms again`,
    );
  },

  async S15({ note }) {
    await openRoom("source");
    await command("source", "/history 74");
    const opened = await reopenRoom("source");
    const first = await waitEvent(
      opened,
      (entry) => entry.via === "messages_query" && entry.room === "source",
      "a messages_query of the source room",
    );
    expect(first.kind === "Before" && anchorCursor(first) === undefined, `the first page asked ${describeAnchor(first)}`);
    const answer = await waitAnswer(opened, first);
    const nodes = await waitScreen(
      (screen) => findText(screen, "History message 00074") && findText(screen, "History message 00073") && screen,
      "History message 00074 and 00073 on screen",
    );
    const newest = findText(nodes, "History message 00074");
    const older = findText(nodes, "History message 00073");
    expect(newest.center.y > older.center.y, "00074 is not below 00073");
    note(`\`${await screenshot(scenarioById("S15"), "source")}\``);
    note(
      `after reopen the first messages_query is Before { cursor: None } (seq ${first.seq}), answered ${answer.kind} with ${pageMessages(answer).length} messages; "History message 00074" is the lowest bubble, below 00073`,
    );
  },

  async S17({ note }) {
    const mark = eventMark();
    let quote;
    for (let attempt = 0; attempt < 30 && !quote; attempt += 1) {
      const screen = parseUiNodes(uiDump(serial));
      quote = replyQuote(screen, "History message 00050", "History message 00040") ??
        replyQuote(screen, "History message 00025", "History message 00015");
      if (!quote) await scrollOlder(600);
    }
    expect(quote, "found no reply bubble of 00050 or 00025 with its quote");
    const target = quote.target;
    await screenshot(scenarioById("S17"), "before");
    inputTap(serial, quote.node.center.x, quote.node.center.y);
    const neighbours = [
      Number(target.slice(-5)) - 1,
      Number(target.slice(-5)) + 1,
    ].map((index) => `History message ${String(index).padStart(5, "0")}`);
    const after = await waitScreen(
      (screen) => neighbours.some((text) => findText(screen, text)) && screen,
      `the messages around ${target} on screen`,
    );
    const asked = eventsSince(mark).filter((entry) => entry.via === "messages_query");
    note(`\`${await screenshot(scenarioById("S17"), "source")}\``);
    note(
      `tapped the quote "${quote.node.text}" in the reply ${quote.reply}; the feed moved to ${target} (${neighbours.filter((text) => findText(after, text)).join(", ")} on screen); ${asked.length} page(s) asked on the way (${asked.map(describeAnchor).join(", ") || "the target was in the window"})`,
    );
  },

  async S16({ note }) {
    const opened = await reopenRoom("source");
    let last;
    for (let attempt = 0; attempt < 40; attempt += 1) {
      last = finalPage(opened);
      if (last && findText(parseUiNodes(uiDump(serial)), "History message 00001")) break;
      await scrollOlder(900, 300);
    }
    const queries = eventsSince(opened).filter(
      (entry) => entry.via === "messages_query" && entry.room === "source",
    );
    const anchors = queries.map(describeAnchor);
    const repeated = anchors.filter((anchor, index) => anchors.indexOf(anchor) !== index);
    expect(last, `no page answered next: None after ${queries.length} queries (${anchors.join(", ")})`);
    expect(findText(parseUiNodes(uiDump(serial)), "History message 00001"), "History message 00001 is not on screen");
    expect(repeated.length === 0, `pages asked twice: ${repeated.join(", ")}`);
    note(`\`${await screenshot(scenarioById("S16"), "source")}\``);
    note(`pages asked in order: ${anchors.join(", ")}; the last answer has next: None; no page asked twice; "History message 00001" on screen`);
  },

  async S18({ note }) {
    await reopenRoom("source");
    await command("source", "/post text");
    const id = await newestPlainText("source");
    const text = plainText(id);
    await findInFeed(text);
    note(`New: "Plain text message ${id.slice(-4)}" (${id})`);
    await command("source", `/edit ${id} Edited in the source room`, { reply: true });
    await waitScreen(
      (screen) => findText(screen, "Edited in the source room") && !findText(screen, text),
      "the edited text in place",
    );
    note("Edited: the bubble shows the new text in place");
    await command("source", `/react ${id} 👍`, { reply: true });
    const reacted = await waitScreen((screen) => reactionChip(screen, "Edited in the source room", "👍"), "a 👍 chip");
    note(`ReactionPlaced: chip "${reacted.text || reacted.description}"`);
    await screenshot(scenarioById("S18"), "react");
    await command("source", `/unreact ${id} 👍`, { reply: true });
    await waitScreen((screen) => !reactionChip(screen, "Edited in the source room", "👍") && screen, "no 👍 chip");
    note("ReactionRemoved: the chip goes");
    await command("source", `/status ${id} read`, { reply: true });
    note("StatusChanged: answered ok");
    await command("source", `/delete ${id}`, { reply: true });
    await waitScreen((screen) => !findText(screen, "Edited in the source room") && screen, "the deleted bubble gone");
    note("Deleted: the bubble goes");
    note(`\`${await screenshot(scenarioById("S18"), "source")}\``);
  },

  async S11({ note }) {
    await openRoom("source");
    const mark = eventMark();
    const text = "A message whose status the product changes";
    await typeAndSend(text);
    const action = await waitEvent(mark, (entry) => isNewMessage(entry, text), `user_action NewMessage "${text}"`);
    const created = await waitAnswer(mark, action);
    const id = created.data?.messageId;
    expect(created.kind === "MessageCreated" && id, `the send was answered ${created.kind}`);
    const shots = [];
    for (const status of ["delivered", "read", "failed"]) {
      await command("source", `/status ${id} ${status}`, { reply: true });
      await findInFeed(text);
      shots.push(await screenshot(scenarioById("S11"), status));
    }
    const menu = await openMessageMenu(text);
    const labels = menuLabels(menu);
    const retry = findText(menu, "Retry");
    shots.push(await screenshot(scenarioById("S11"), "menu"));
    expect(retry, `the menu of the Failed message offers no Retry (${labels.join(", ")})`);
    const retried = eventMark();
    inputTap(serial, retry.center.x, retry.center.y);
    const again = await waitEvent(retried, (entry) => isRetry(entry), "user_action Retry");
    const answer = await waitAnswer(retried, again);
    expect(answer.kind === "Acknowledged", `Retry was answered ${answer.kind}`);
    note(shots.map((shot) => `\`${shot}\``).join(", "));
    note(
      `/status ${id} delivered, read, failed on the user's message answered ok (status marks in the screenshots); the menu of the Failed message offered ${labels.join(", ")}; Retry sent user_action Retry ${JSON.stringify(again.data)}, answered ${answer.kind}`,
    );
  },

  async S19({ note }) {
    await openRoom("source");
    await command("source", "/slow 8000", { reply: true });
    const mark = eventMark();
    const text = "Hello from the emulator";
    await typeAndSend(text);
    // The screenshot and the screen come first. When the event log has no answer after them, they show the send before the answer.
    const shot = await screenshot(scenarioById("S19"), "preparing", 0);
    const preparing = findText(parseUiNodes(uiDump(serial)), text);
    const seenAtPreparing = events();
    const action = await waitEvent(mark, (entry) => isNewMessage(entry, text), `user_action NewMessage "${text}"`);
    const answer = await waitAnswer(mark, action, ANSWER_TIMEOUT_MS);
    expect(answer.kind === "MessageCreated", `the answer is ${answer.kind}`);
    const pending = !seenAtPreparing.some((entry) => entry.via === "answer" && entry.re === action.seq && entry.boot === action.boot);
    expect(pending && preparing, "the bubble did not show before the answer");
    await delay(1_000);
    expect(findText(parseUiNodes(uiDump(serial)), text), `"${text}" left the feed after MessageCreated`);
    const created = await screenshot(scenarioById("S19"), "source");
    await command("source", "/slow 0", { reply: true, timeoutMs: ANSWER_TIMEOUT_MS });
    note(`\`${shot}\`, \`${created}\``);
    note(
      `under /slow 8000 the bubble showed with the Preparing mark (\`s19-preparing.png\`) before the product answered; the answer MessageCreated ${answer.data?.messageId ?? JSON.stringify(answer.data)} replaced it and the bubble stayed (\`s19-source.png\`)`,
    );
  },

  async S20({ note }) {
    await openRoom("source");
    await command("source", "/fail reject");
    const mark = eventMark();
    const text = "This send is rejected";
    const sent = await typeAndSend(text);
    // The app shows the reason in a notice for 2.5 s, so the screen is read at once.
    const notice = (nodes) => nodes.find((node) => node.text.includes("Rejected by /fail"));
    let reason = notice(sent);
    let reasonShot;
    const deadline = Date.now() + 8_000;
    while (Date.now() < deadline && !reasonShot) {
      const screen = parseUiNodes(uiDump(serial));
      reason ??= notice(screen);
      if (notice(screen)) reasonShot = await screenshot(scenarioById("S20"), "reason", 0);
      else if (reason) break;
    }
    const action = await waitEvent(mark, (entry) => isNewMessage(entry, text), `user_action NewMessage "${text}"`);
    const answer = await waitAnswer(mark, action);
    expect(answer.kind === "Rejected", `the answer is ${answer.kind}`);
    await waitScreen((screen) => !findText(screen, text) && screen, `"${text}" removed`);
    const shot = await screenshot(scenarioById("S20"), "source");
    expect(reason, 'the reason "Rejected by /fail" never showed on screen');
    note(reasonShot ? `\`${reasonShot}\`, \`${shot}\`` : `\`${shot}\``);
    note(`answer Rejected { reason: "Rejected by /fail" }; the bubble was removed and the screen showed "${reason.text}"`);
  },

  async S21({ note }) {
    await openRoom("source");
    await command("source", "/fail timeout");
    const mark = eventMark();
    const text = "This send gets no answer";
    await typeAndSend(text);
    const action = await waitEvent(mark, (entry) => isNewMessage(entry, text), `user_action NewMessage "${text}"`);
    const started = Date.now();
    const timeout = await waitAnswer(mark, action, 60_000);
    const waited = Date.now() - started;
    expect(timeout.kind === "Timeout", `the answer is ${timeout.kind}`);
    await delay(1_000);
    const menu = await openMessageMenu(text);
    const retry = findText(menu, "Retry");
    expect(retry, `the menu of the failed send offers no Retry (${menuLabels(menu).join(", ")})`);
    const failedShot = await screenshot(scenarioById("S21"), "failed");
    const retried = eventMark();
    inputTap(serial, retry.center.x, retry.center.y);
    const again = await waitEvent(retried, (entry) => isNewMessage(entry, text) || isRetry(entry), "a resend after Retry");
    const answer = await waitAnswer(retried, again);
    expect(["MessageCreated", "Acknowledged"].includes(answer.kind), `the resend was answered ${answer.kind}`);
    note(`\`${failedShot}\`, \`${await screenshot(scenarioById("S21"), "source")}\``);
    note(
      `no answer: the host stopped the query after ${Math.round(waited / 1000)} s (event log Timeout), the menu offered Retry; Retry sent ${again.kind} again, answered ${answer.kind}`,
    );
  },

  async S46({ note }) {
    // A reopen asks the participant count before the first page, and the count would take the interrupt. So the
    // room stays open, and a scroll to older messages asks the next query.
    await reopenRoom("source");
    await command("source", "/fail unavailable");
    const mark = eventMark();
    let first;
    for (let attempt = 0; attempt < 20 && !first; attempt += 1) {
      await scrollOlder(900, 300);
      first = eventsSince(mark).find((entry) => entry.via.endsWith("_query"));
    }
    expect(first?.via === "messages_query", `the next query after /fail unavailable is ${first ? `${first.via} ${first.kind}` : "none"}`);
    const refused = await waitAnswer(mark, first);
    expect(refused.kind === "NotAvailable", `the first answer is ${refused.kind}`);
    const second = await waitEvent(
      mark,
      (entry) => entry.via === "messages_query" && entry.seq > first.seq,
      "the retry of the messages_query",
    );
    const answer = await waitAnswer(mark, second);
    expect(answer.kind === "Page", `the retry was answered ${answer.kind}`);
    expect(describeAnchor(second) === describeAnchor(first), "the retry asked another page");
    note(`\`${await screenshot(scenarioById("S46"), "source")}\``);
    await scrollToNewest();
    note(
      `with the room open, the scroll to older messages asked ${describeAnchor(first)}, answered NotAvailable; the host asked the same page once more and got ${answer.kind} with ${pageMessages(answer).length} messages`,
    );
  },

  async S22({ note }) {
    await openRoom("source");
    await command("source", "/history 560");
    const opened = await reopenRoom("source");
    // The new history is newer than the message that the user saw last, so the room opens at the "New messages"
    // marker, not at the newest message. The launcher scrolls to the newest message first.
    for (let attempt = 0; attempt < 10 && !findText(parseUiNodes(uiDump(serial)), "History message 00560"); attempt += 1) {
      await scrollToNewest();
    }
    await waitScreen((screen) => findText(screen, "History message 00560"), "History message 00560");
    let pages = [];
    for (let attempt = 0; attempt < 60; attempt += 1) {
      pages = sourcePages(opened);
      if (finalPage(opened) || pages.length >= 12) break;
      await scrollOlder(1_200, 60);
    }
    const loaded = pages.reduce((sum, page) => sum + page.count, 0);
    expect(loaded > 500, `only ${loaded} messages paged in`);
    const turned = eventMark();
    for (let attempt = 0; attempt < 60; attempt += 1) {
      if (findText(parseUiNodes(uiDump(serial)), "History message 00560")) break;
      await scrollNewer(1_200, 60);
    }
    expect(findText(parseUiNodes(uiDump(serial)), "History message 00560"), "History message 00560 never came back");
    const again = eventsSince(turned).filter((entry) => entry.via === "messages_query" && entry.room === "source");
    expect(again.length > 0, "scrolling back to the newest message asked no page: the window kept all the messages");
    note(`\`${await screenshot(scenarioById("S22"), "source")}\``);
    note(
      `paged ${loaded} messages older in ${pages.length} pages; scrolling back to 00560 asked ${again.length} dropped page(s) again (${again.map(describeAnchor).join(", ")}), so the window dropped the newest messages instead of growing past 500`,
    );
  },

  async S24({ note }) {
    const opened = await reopenRoom("source");
    const first = await waitEvent(opened, (entry) => isRoomEvent(entry, "source", "RoomOpened"), "RoomOpened on open");
    await waitFor(async () => (await workerPages()) === 1, {
      timeoutMs: 20_000,
      intervalMs: 1_000,
      message: "no worker page runs while the room is open (DevTools page list)",
    });
    await delay(3_000);

    const background = eventMark();
    inputKeyEvent(serial, KEY.home);
    await waitEvent(background, (entry) => isRoomEvent(entry, "source", "RoomClosed"), "RoomClosed on background");
    await delay(6_000);
    const spurious = eventsSince(background).filter((entry) => isRoomEvent(entry, "source", "RoomOpened"));
    expect(spurious.length === 0, `the product got RoomOpened while the app was in the background (boot ${spurious[0]?.boot})`);

    const foreground = eventMark();
    amStart(serial, ["-n", launcherActivity]);
    const returned = await waitEvent(foreground, (entry) => isRoomEvent(entry, "source", "RoomOpened"), "RoomOpened on return");
    await delay(3_000);

    const closed = eventMark();
    inputKeyEvent(serial, KEY.back);
    currentRoom = undefined;
    await waitEvent(closed, (entry) => isRoomEvent(entry, "source", "RoomClosed"), "RoomClosed on leaving the room");
    const started = Date.now();
    await waitFor(async () => (await workerPages()) === 0, {
      timeoutMs: 90_000,
      intervalMs: 2_000,
      message: "the worker page still runs 90 s after the room closed",
    });
    const stoppedAfter = Math.round((Date.now() - started) / 1000);
    note(`\`${await screenshot(scenarioById("S24"), "list")}\``);
    note(
      `event log: RoomOpened on open (boot ${first.boot}), RoomClosed on Home and no event in the background, RoomOpened on return (boot ${returned.boot}${returned.boot !== first.boot ? ", a new worker: it stopped in the background" : ""}), RoomClosed on Back; the worker WebView page was gone ${stoppedAfter} s after the close (DevTools page list)`,
    );
  },

  async S23({ note }) {
    await openRoom("source");
    await waitScreen((screen) => findText(screen, "History message 00560"), "the cached newest page");
    await command("source", "/fail crash");
    const crashMark = logMark();
    const text = "This send crashes the worker";
    await typeAndSend(text);
    const crashed = await waitForLog(
      crashMark,
      (line) => line.tag === "ProductWebChromeClientKt" && line.message.includes("/fail crash: the sample product stops its worker"),
      "the uncaught /fail crash error",
    );
    note(`/fail crash threw "${crashed.message.replace(/ \[.*$/, "").slice(0, 120)}", which a WebView page survives`);
    const boots = new Set(events().map((entry) => entry.boot));
    const killed = killWorkerRenderer();
    note(`killed the WebView renderer ${killed}`);
    const cached = await waitScreen(
      (screen) => findText(screen, "History message 00560") && screen,
      "the cached messages after the kill",
      10_000,
    );
    expect(cached, "the cached page left the screen");
    let unavailable;
    await waitFor(
      () => {
        const screen = parseUiNodes(uiDump(serial));
        unavailable = screen.find((node) => node.text.startsWith("The app does not answer"));
        return unavailable || events().some((entry) => !boots.has(entry.boot));
      },
      { timeoutMs: ANSWER_TIMEOUT_MS, intervalMs: 1_000, message: "neither the unavailable line nor a new worker boot" },
    );
    const killedShot = await screenshot(scenarioById("S23"), "killed");
    if (unavailable) {
      const retry = parseUiNodes(uiDump(serial)).find((node) => node.text === "Retry" && node.center.y > unavailable.bounds.top - 40);
      if (retry) inputTap(serial, retry.center.x, retry.center.y);
    }
    const reboot = await waitEvent(0, (entry) => !boots.has(entry.boot), "a new worker boot", ANSWER_TIMEOUT_MS);
    await command("source", "/slow 0", { reply: true, timeoutMs: ANSWER_TIMEOUT_MS });
    note(`\`${killedShot}\`, \`${await screenshot(scenarioById("S23"), "source")}\``);
    note(
      `the cached page stayed on screen; ${unavailable ? `the room showed "${unavailable.text}" and Retry` : "the room showed no unavailable line"}; the worker came back as boot ${reboot.boot} and answered the next command`,
    );
  },

  async S1({ note }) {
    await openRoom("host");
    const before = messages("host");
    await command("host", "/post legacy");
    const legacy = await waitFor(
      () => addedMessages("host", before).find((message) => message.text === "Legacy message from post_message"),
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "no legacy message stored" },
    );
    expect(!legacy.id.includes("/"), `the legacy message has a product id ${legacy.id}`);
    await waitScreen((screen) => findText(screen, "Legacy message from post_message"), "the legacy bubble");
    note(`\`${await screenshot(scenarioById("S1"), "host")}\``);
    note(`chat_messages: "${legacy.text}" type ${legacy.type} with the host id ${legacy.id}, from the extension`);
  },

  async S3({ note }) {
    if (roomRow("host")?.name !== ROOMS.host.name) {
      await command("host", `/meta ${ROOMS.host.name}`, { reply: true });
      await reopenRoom("host");
    }
    await openRoom("host");
    await waitScreen((screen) => findText(screen, ROOMS.host.name), `the header named "${ROOMS.host.name}"`);
    try {
      await command("host", "/meta Renamed by the launcher", { reply: true });
      await waitFor(() => roomRow("host")?.name === "Renamed by the launcher", {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: "chat_rooms.name did not change",
      });
      note('chat_rooms.name became "Renamed by the launcher"');
      await waitScreen(
        (screen) => findText(screen, "Renamed by the launcher") && findText(screen, "Renamed by /meta") && screen,
        "the new name and description in the open header",
      );
      note('the open header shows the new name and the description "Renamed by /meta" without a reopen');
      note(`\`${await screenshot(scenarioById("S3"), "host")}\``);
      await showChatList();
      const list = await waitScreen((screen) => findText(screen, "Renamed by the launcher") && screen, "the new name in the chat list");
      expect(list, "the list row kept the old name");
      note("the list row shows the new name");
    } finally {
      await command("host", `/meta ${ROOMS.host.name}`, { reply: true });
    }
  },

  // S49: the product reads from `capabilities` exactly the names that the app
  // lists (the app logs them as it answers), and both attachment bounds.
  async S49({ note }) {
    await openRoom("host");
    const before = messages("host");
    const mark = logMark();
    await command("host", "/caps");
    const answer = await waitFor(
      () => addedMessages("host", before).find((message) => message.text.startsWith("caps"))?.text,
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "no capabilities message stored" },
    );
    const logged = await waitForLog(
      mark,
      (line) => line.message.startsWith("truapi.chat.capabilities "),
      "the capabilities that the app lists",
    );
    const listed = logged.message.split(": ").slice(1).join(": ").split(",").filter(Boolean);
    const match = /caps: (.*); inline (\d+) B; preimage (\d+) B/.exec(answer);
    expect(match, `no capabilities in "${answer}"`);
    const read = match[1] === "none" ? [] : match[1].split(", ");
    expect(
      [...read].sort().join(",") === [...listed].sort().join(","),
      `the product read ${read.join(",") || "none"}, the app lists ${listed.join(",") || "none"}`,
    );
    expect(match[2] === String(1024 * 1024) && match[3] === "0", `bounds inline ${match[2]} B, preimage ${match[3]} B`);
    listedFeatures = listed;
    await waitScreen((screen) => findText(screen, /^caps: /), "the capabilities message");
    note(`\`${await screenshot(scenarioById("S49"), "host")}\``);
    note(
      `the app lists ${listed.join(", ") || "none"}; the product read ${read.join(", ") || "none"}; inline ${match[2]} B, preimage ${match[3]} B; the rows of the listed names: ${listed.map((name) => `${name} ${ANDROID_CHAT_FEATURE_ROWS[name]?.join(", ") ?? "none"}`).join("; ") || "none"}`,
    );
  },

  // S53: a persona of the product draws with its name and icon in the host
  // room, with no participants query, also in the activity line; a repeat
  // renames it, and an unknown persona is refused.
  async S53({ note }) {
    await openRoom("host");
    const asked = eventMark();
    const before = messages("host");
    await command("host", "/bot add helper Helper");
    const registered = await waitFor(
      () => addedMessages("host", before).find((message) => /^bot helper (New|Exists)$/.test(message.text))?.text,
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "no answer to /bot add" },
    );
    const stored = await waitFor(() => personaRows().find((row) => row.botId === "helper"), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "chat_bots has no persona helper",
    });
    expect(stored.name === "Helper", `chat_bots names helper "${stored.name}"`);
    note(`/bot add helper Helper: the product read "${registered}"; chat_bots keeps helper "${stored.name}"`);

    const posted = messages("host");
    await command("host", "/bot post helper Hello from the persona");
    const spoken = await waitFor(
      () => addedMessages("host", posted).find((message) => message.text === "Hello from the persona"),
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "the persona message is not stored" },
    );
    expect(spoken.participant === "bot:helper", `the persona message is stored as "${spoken.participant}"`);
    await scrollToNewest();
    await waitScreen((screen) => authorAbove(screen, "Hello from the persona", "Helper") && screen, 'the author "Helper" above the persona message');
    note(`\`${await screenshot(scenarioById("S53"), "author")}\``);

    const renamed = messages("host");
    await command("host", "/bot add helper Helper Two");
    await waitFor(() => addedMessages("host", renamed).some((message) => message.text === "bot helper Exists"), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: 'no "bot helper Exists" after the repeat',
    });
    await waitFor(() => personaRows().find((row) => row.botId === "helper")?.name === "Helper Two", {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "chat_bots did not take the new name",
    });
    await scrollToNewest();
    await waitScreen((screen) => authorAbove(screen, "Hello from the persona", "Helper Two") && screen, 'the new name "Helper Two" above the persona message');

    await command("host", "/bot typing helper", { reply: true });
    const typing = await waitScreen((screen) => findText(screen, "Helper Two: Sample is typing") && screen, 'the activity line "Helper Two: Sample is typing"');
    expect(typing, "no activity line");
    note(`\`${await screenshot(scenarioById("S53"), "activity")}\``);
    await command("host", "/activity off", { reply: true });

    const ghost = messages("host");
    await command("host", "/bot post ghost Boo");
    const refused = await waitFor(
      () => addedMessages("host", ghost).find((message) => message.text.startsWith("error /bot post ghost Boo"))?.text,
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unknown persona was not refused" },
    );
    expect(/unknown bot ghost/.test(refused), `the refusal reads "${refused}"`);
    const queried = eventsSince(asked).filter((entry) => entry.via === "participants_query");
    expect(queried.length === 0, `the app asked participants_query ${queried.length} time(s)`);
    note(
      `the persona message is stored with originParticipantId bot:helper and drawn under the author "Helper" with no participants_query; the repeat answered Exists and renamed it to "Helper Two"; the activity line reads "Helper Two: Sample is typing"; /bot post ghost was refused: "${refused}"`,
    );
  },

  // S64: a thread chip under the root opens the thread room, which the list
  // does not show; Back returns to the root.
  async S64({ note }) {
    await openRoom("host");
    await command("host", "/bot add helper Helper");
    await command("host", "/post text");
    const root = await newestPlainText("host");
    await command("host", `/thread ${root}`, { reply: true });
    const threadRoom = `thread-host-${root}`;
    const threadChat = chatIdHex(extensionId, threadRoom);
    await waitFor(() => sql(`SELECT id FROM chat_rooms WHERE id = X'${threadChat}'`)[0], {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `chat_rooms has no ${threadRoom}`,
    });
    await waitFor(() => metaRows("host").some((row) => row.messageId === root), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `chat_message_meta has no row for ${root}`,
    });
    await findInFeed(plainText(root));
    const rooted = await waitScreen((screen) => chipUnder(screen, plainText(root)) && screen, `the thread chip under ${root}`);
    const chip = chipUnder(rooted, plainText(root));
    const chipText = rooted.filter((node) => insideOf(node, chip)).map((node) => node.text).join(" ");
    expect(/\b3 replies\b/.test(chipText), `the chip reads "${chipText}"`);
    const unread = rooted.some((node) => node.tag === "chat_thread_unread" && insideOf(node, chip));
    note(`\`${await screenshot(scenarioById("S64"), "root")}\``);

    const list = await showChatList();
    expect(!findText(list, "Thread"), "the chat list shows the thread room");
    await openRoom("host");
    await findInFeed(plainText(root));
    const again = await waitScreen((screen) => chipUnder(screen, plainText(root)) && screen, "the chip after the list");
    const target = chipUnder(again, plainText(root));
    inputTap(serial, target.center.x, target.center.y);
    currentRoom = undefined;
    const thread = await waitScreen(
      (screen) =>
        findText(screen, "Thread") &&
        findText(screen, ROOMS.host.name) &&
        findTag(screen, "chat_thread_root") &&
        findText(screen, "Reply 3 from bot:helper") &&
        screen,
      "the thread room with its header, the pinned root and the replies",
    );
    const bar = findTag(thread, "chat_thread_root");
    expect(thread.some((node) => plainText(root).test(node.text) && insideOf(node, bar)), "the pinned root is not the root message");
    for (const reply of ["Reply 1 from p1", "Reply 2 from p2"]) expect(findText(thread, reply), `no "${reply}"`);
    expect(authorAbove(thread, "Reply 3 from bot:helper", "Helper"), 'the persona reply has no author "Helper"');
    note(`\`${await screenshot(scenarioById("S64"), "thread")}\``);

    inputKeyEvent(serial, KEY.back);
    const back = await waitScreen(
      (screen) => findTag(screen, "chat_message_input") && findText(screen, ROOMS.host.name) && findText(screen, plainText(root)) && screen,
      "the host room at the root after Back",
    );
    expect(back, "Back did not return to the root");
    currentRoom = "host";

    note(
      `/thread ${root}: ${threadRoom} in chat_rooms and a meta row for ${root}; the chip reads "${chipText}"${unread ? " with the unread dot" : ""}; the list has no thread row; a tap opens "Thread" with "${ROOMS.host.name}", the pinned root and the replies from p1, p2 and the persona "Helper"; Back shows the root again. Deleting the parent is not driven: the sample runs commands only in host, source and its thread rooms, so no thread of the throwaway room that /room delete removes can be made (RealDeleteRoomUseCaseTest covers the cascade)`,
    );
  },

  // S65: the branch arrows send BranchSelected; the product's answer moves
  // the position; an answer later than 5 s leaves the control at the stored
  // position, and the late answer then moves it.
  async S65({ note }) {
    await openRoom("host");
    await command("host", "/post text");
    const id = await newestPlainText("host");
    await command("host", `/branches ${id} 3`, { reply: true });
    await findInFeed(plainText(id));
    let screen = await waitScreen((nodes) => branchUnder(nodes, plainText(id))?.position.text === "1/3" && nodes, "the branch control 1/3");
    let mark = eventMark();
    let control = branchUnder(screen, plainText(id));
    inputTap(serial, control.next.center.x, control.next.center.y);
    const selected = await waitEvent(
      mark,
      (entry) => entry.via === "room_events" && entry.room === "host" && entry.kind === "BranchSelected" && Number(entry.data?.index) === 2,
      "BranchSelected { index: 2 }",
    );
    screen = await waitScreen(
      (nodes) => findText(nodes, "Branch 2 of 3") && branchUnder(nodes, /^Branch 2 of 3$/)?.position.text === "2/3" && !findTag(nodes, "chat_branch_waiting") && nodes,
      'the answer "Branch 2 of 3" at 2/3',
    );
    note(`\`${await screenshot(scenarioById("S65"), "answered")}\``);

    // The product answers 15 s late; the app gives up after 5 s and logs it. Screen dumps are slow, so the margin is wide.
    await command("host", "/slow 15000", { reply: true });
    try {
      control = branchUnder(parseUiNodes(uiDump(serial)), /^Branch 2 of 3$/);
      expect(control, "no branch control under Branch 2 of 3");
      mark = eventMark();
      const logged = logMark();
      const tapped = Date.now();
      inputTap(serial, control.next.center.x, control.next.center.y);
      await waitScreen((nodes) => findTag(nodes, "chat_branch_waiting") && nodes, "the spinner of the control", 4_000);
      note(`\`${await screenshot(scenarioById("S65"), "waiting", 200)}\``);
      const gaveUp = await waitForLog(logged, (line) => /^truapi\.chat\.branch .*: no answer for 3 in 5000 ms$/.test(line.message), "the 5 s time-out of the control", 12_000);
      note(`the control gave up ${((Date.now() - tapped) / 1000).toFixed(1)} s after the tap ("${gaveUp.message}")`);
      const reverted = await waitScreen(
        (nodes) => !findTag(nodes, "chat_branch_waiting") && branchUnder(nodes, /^Branch 2 of 3$/)?.position.text === "2/3" && nodes,
        "the control back at 2/3 with no spinner",
        3_000,
      );
      expect(!findTag(reverted, "chat_branch_waiting"), "the spinner is still there after 5 s");
      expect(branchUnder(reverted, /^Branch 2 of 3$/)?.position.text === "2/3", "the control is not back at 2/3 after 5 s");
      note(`\`${await screenshot(scenarioById("S65"), "reverted", 100)}\``);
      expect(Date.now() - tapped < 15_000, "the check of the revert came after the late answer");
      await waitScreen((nodes) => branchUnder(nodes, /^Branch 3 of 3$/)?.position.text === "3/3" && nodes, "the late answer at 3/3", 30_000);
      await waitEvent(mark, (entry) => entry.via === "room_events" && entry.kind === "BranchSelected" && Number(entry.data?.index) === 3, "BranchSelected { index: 3 }");
    } finally {
      await command("host", "/slow 0", { reply: true, timeoutMs: 45_000 });
    }
    note(
      `a tap on › sent BranchSelected ${JSON.stringify(selected.data)}; the product's Edited "Branch 2 of 3" and meta moved the control to 2/3; with /slow 15000 the next tap showed the spinner, after 5 s the control was back at 2/3 with no spinner, and the late answer moved it to 3/3`,
    );
  },

  // S66: a reasoning row reads "Thinking…" while the message is Preparing,
  // then "Thought for 4 s" collapsed; a tap expands it; TalkBack reads its
  // state; it is never the list preview.
  async S66({ note }) {
    await openRoom("host");
    await scrollToNewest();
    const before = messages("host");
    await typeAndSend("/reason");
    const states = new Set();
    let thinkingShot;
    const started = Date.now();
    while (Date.now() - started < 20_000) {
      const screen = parseUiNodes(uiDump(serial));
      const row = screen.find((node) => /^Reasoning, (expanded|collapsed)$/.test(node.description));
      if (row?.description === "Reasoning, expanded" && findText(screen, "Thinking…")) {
        if (!states.has("thinking")) thinkingShot = await screenshot(scenarioById("S66"), "thinking", 0);
        states.add("thinking");
        if (screen.some((node) => node.tag === "chat_reasoning_text" && /^The user/.test(node.text))) states.add("streamed");
      }
      if (row && findText(screen, "Thought for 4 s") && row.description === "Reasoning, collapsed") {
        states.add("collapsed");
        break;
      }
    }
    expect(states.has("collapsed"), `the row did not collapse to "Thought for 4 s" (seen: ${[...states].join(", ") || "nothing"})`);
    // The stored search text of a message keeps its first body until P8.9, so it is "The".
    const reasoning = addedMessages("host", before).find((message) => /^The( user asked|$)/.test(message.text));
    expect(reasoning, "no reasoning message stored");
    const collapsed = parseUiNodes(uiDump(serial));
    expect(!collapsed.some((node) => node.tag === "chat_reasoning_text"), "the collapsed row shows its text");
    note(`\`${await screenshot(scenarioById("S66"), "collapsed")}\``);
    const header = collapsed.find((node) => node.description === "Reasoning, collapsed");
    inputTap(serial, header.center.x, header.center.y);
    const expanded = await waitScreen(
      (screen) => screen.some((node) => node.description === "Reasoning, expanded") && screen.some((node) => node.tag === "chat_reasoning_text" && /The user asked/.test(node.text)) && screen,
      "the expanded row with its text",
    );
    expect(expanded, "no expanded row");
    note(`\`${await screenshot(scenarioById("S66"), "expanded")}\``);
    const list = await showChatList();
    const row = rowNodes(list, ROOMS.host.name);
    expect(!row.some((node) => /The user asked/.test(node.text)), `the list preview is the reasoning row: ${row.map((node) => node.text).join(" | ")}`);
    note(`\`${await screenshot(scenarioById("S66"), "list")}\``);
    note(
      `${states.has("thinking") ? `"Thinking…" with the streamed text (\`${thinkingShot}\`), then ` : "the Thinking state was shorter than one screen dump; "}"Thought for 4 s" with the content description "Reasoning, collapsed"; a tap gives "Reasoning, expanded" and the text; the host row of the list shows "${row.map((node) => node.text).filter(Boolean).join(" | ")}", not the reasoning row`,
    );
  },

  // S67: Read aloud is in the menu only with read_aloud, speaks RichText as
  // plain text, and stops when the user leaves the room.
  async S67({ note }) {
    await openRoom("host");
    await command("host", "/features readaloud off", { reply: true });
    await command("host", "/post rich");
    await waitFor(() => messages("host").some((message) => /^# Rich text sample/.test(message.text)), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "the rich message is not stored",
    });
    const without = await openRichMenu();
    expect(!findText(without, "Read aloud"), "Read aloud without read_aloud");
    inputKeyEvent(serial, KEY.back);
    await delay(800);

    await command("host", "/features readaloud on", { reply: true });
    await delay(1_500);
    const menu = await openRichMenu();
    expect(findText(menu, "Read aloud"), `no Read aloud with read_aloud; the menu shows ${menuLabels(menu).join(", ")}`);
    note(`\`${await screenshot(scenarioById("S67"), "menu", 300)}\``);
    const logged = logMark();
    tapText(menu, "Read aloud");
    const spoke = await waitForLog(logged, (line) => line.message.startsWith("truapi.chat.readAloud speak "), "the speak line");
    const match = /speak (\d+) characters, marks (true|false)/.exec(spoke.message);
    expect(match && match[2] === "false", `the spoken text has Markdown marks: ${spoke.message}`);
    const speaking = await waitScreen((screen) => findTag(screen, "chat_read_aloud_stop") && screen, 'the "Stop" line while it speaks', 10_000).catch(() => undefined);
    const engine = logLines(logged).find((line) => /truapi\.chat\.readAloud (unavailable|failed|done)/.test(line.message));
    if (speaking) note(`\`${await screenshot(scenarioById("S67"), "speaking", 200)}\``);
    inputKeyEvent(serial, KEY.back);
    currentRoom = undefined;
    const stopped = await waitForLog(logged, (line) => /truapi\.chat\.readAloud (stop|done|unavailable|failed)/.test(line.message), "the end of the speech", 10_000);
    expect(speaking, `no Stop line while it speaks${engine ? `: ${engine.message}` : ""}`);
    expect(/readAloud stop$/.test(stopped.message), `the speech ended with "${stopped.message}" before the user left`);
    await delay(1_000);
    const after = parseUiNodes(uiDump(serial));
    expect(!findTag(after, "chat_read_aloud_stop"), "the Stop line is still on screen after leaving");
    await command("host", "/features readaloud off", { reply: true });
    note(
      `without read_aloud the menu of the rich message has no Read aloud; with it, Read aloud spoke ${match[1]} characters with no Markdown marks (logcat ${JSON.stringify(spoke.message)}), the footer showed Stop, and Back stopped the speech ("${stopped.message}")`,
    );
  },

  // S68: the meta of a message of the product-source room is kept by the
  // host, so its branch control comes back when the room loads its pages again.
  async S68({ note }) {
    await openRoom("source");
    await command("source", "/post text");
    const id = await newestPlainText("source");
    await command("source", `/branches ${id} 3`, { reply: true });
    await waitFor(() => metaRows("source").some((row) => row.messageId === id), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `chat_message_meta has no row for ${id}`,
    });
    await findInFeed(plainText(id));
    await waitScreen((nodes) => branchUnder(nodes, plainText(id))?.position.text === "1/3" && nodes, "the branch control 1/3");
    note(`\`${await screenshot(scenarioById("S68"), "before")}\``);
    const reloaded = eventMark();
    await reopenRoom("source");
    await waitEvent(reloaded, (entry) => entry.via === "messages_query" && entry.room === "source", "a new messages_query page");
    await findInFeed(plainText(id));
    await waitScreen((nodes) => branchUnder(nodes, plainText(id))?.position.text === "1/3" && nodes, "the branch control after the reload");
    note(`\`${await screenshot(scenarioById("S68"), "after")}\``);
    note(`/branches ${id} 3 in the product-source room: a chat_message_meta row; after the room closed and loaded its pages again through messages_query, the control 1/3 is back under the message`);
  },

  // S61: the host searches the messages it keeps, without case and
  // diacritics, and a tap on a result jumps to the message.
  async S61({ note }) {
    await openRoom("host");
    const text = "Crème brûlée at the CAFÉ";
    await typeAndSend(text);
    await waitFor(() => messages("host").some((message) => message.text === text), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: `"${text}" is not stored`,
    });
    // Newer messages push the match out of the screen, so the tap must jump to it.
    const posted = messages("host");
    await command("host", "/post all");
    await waitFor(() => addedMessages("host", posted).length >= 8, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the /post all messages are not stored" });
    await delay(2_000);
    await scrollToNewest();
    const before = parseUiNodes(uiDump(serial));
    expect(!findText(before, text), `"${text}" is still on screen after /post all`);

    const found = {};
    for (const query of ["creme BRULEE", "café"]) {
      const screen = await searchInRoom(query);
      const hit = screen.find((node) => /Crème brûlée at the CAFÉ/.test(node.text));
      expect(hit, `no result "${text}" for "${query}"; the screen shows ${screenSummary(screen)}`);
      found[query] = hit;
      note(`\`${await screenshot(scenarioById("S61"), query.replaceAll(" ", "-").toLowerCase())}\``);
    }
    inputTap(serial, found["café"].center.x, found["café"].center.y);
    const jumped = await waitScreen(
      (screen) => !findTag(screen, "chat_room_search_field") && findText(screen, text) && screen,
      `"${text}" in the feed after the tap`,
    );
    const bubble = findText(jumped, text);
    expect(bubble.center.y > 300 && bubble.center.y < 2100, `"${text}" is outside the feed at y ${bubble.center.y}`);
    note(`\`${await screenshot(scenarioById("S61"), "jump")}\``);
    note(
      `the user's "${text}" is found by "creme BRULEE" and by "café" (case and diacritics ignored); a tap closes the field and the feed shows it at y ${Math.round(bubble.center.y)} after /post all pushed it out of the screen. A host-store room loads all its messages, so the jump is a scroll, not a page load`,
    );
  },

  // S54: the user mutes the room for one hour from the list; the row shows
  // the mute and a grey count, /notify into the room shows nothing (now and
  // later), and the product hears of the mute.
  async S54({ note }) {
    await resetUserState("host");
    await showChatList();
    const muted = eventMark();
    const asked = Date.now();
    await swipeRow(ROOMS.host.name, "mute", muteSheetShown);
    await tapWhenShown("For 1 hour", "the mute durations");
    const stored = await waitFor(() => {
      const row = userStateRow("host");
      return row?.mutedUntil ? row : undefined;
    }, { timeoutMs: COMMAND_TIMEOUT_MS, message: "chat_room_declarations.mutedUntil is not set" });
    const hours = (Number(stored.mutedUntil) - asked) / 3_600_000;
    expect(hours > 0.95 && hours < 1.05, `mutedUntil is ${hours.toFixed(3)} h from now`);
    // The core keeps a room event until the worker runs. A new message of the product starts the worker and gives
    // the row an unread count.
    await hookPost("host", "/post text");
    const event = await waitEvent(
      muted,
      (entry) => entry.via === "room_events" && entry.room === "host" && entry.kind === "UserStateChanged",
      "UserStateChanged for the host room",
    );
    expect(String(event.data?.mutedUntil) === String(stored.mutedUntil), `the product got mutedUntil ${event.data?.mutedUntil}, stored ${stored.mutedUntil}`);
    note(`chat_room_declarations.mutedUntil = now + ${hours.toFixed(3)} h; the product got UserStateChanged ${JSON.stringify(event.data)} (seq ${event.seq})`);

    const list = await waitScreen(
      (screen) => rowNodes(screen, ROOMS.host.name).some((node) => node.tag === "chat_list_mute_icon") &&
        rowNodes(screen, ROOMS.host.name).some((node) => node.tag === "chat_list_muted_badge") && screen,
      "the mute icon and the grey count in the host row",
    );
    expect(list, "no muted row");
    note(`\`${await screenshot(scenarioById("S54"), "list")}\``);
    note("the host row shows the speaker icon and the grey count (chat_list_mute_icon, chat_list_muted_badge)");

    // The notification checks come last: the product must be able to send a notification at all, and a notification
    // that a chat worker sends needs the notifications permission of the product first.
    await notifyFromRoom("/notify", { mutedRoom: true });
    const shownBefore = productNotifications();
    await notifyFromRoom("/notify 3");
    await delay(8_000);
    const shownMuted = productNotifications();
    expect(shownMuted === shownBefore, `${shownMuted - shownBefore} notification(s) of the product showed while the room is muted`);
    note("/notify and /notify 3 into the muted room: the product got ok for both, and no notification showed in 8 s (dumpsys notification)");

    await showChatList();
    const unmuted = eventMark();
    await swipeRow(ROOMS.host.name, "mute", () => userStateRow("host")?.mutedUntil == null);
    await waitFor(() => userStateRow("host")?.mutedUntil == null, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unmute is not stored" });
    await notifyFromRoom("/notify");
    await waitEvent(
      unmuted,
      (entry) => entry.via === "room_events" && entry.room === "host" && entry.kind === "UserStateChanged" && entry.data?.mutedUntil == null,
      "UserStateChanged on unmute",
    );
    const shown = await waitFor(() => (productNotifications() > shownMuted ? productNotifications() : undefined), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "/notify after the unmute showed no notification",
    });
    note(`after a swipe unmutes the room, the product gets UserStateChanged with no mute and /notify shows ${shown - shownMuted} notification`);
  },

  // S55: an archived room leaves the main list for the archived section, and
  // stays there when a message arrives; unarchive brings it back.
  async S55({ note }) {
    await resetUserState("host");
    await showChatList();
    const archived = eventMark();
    await swipeRow(ROOMS.host.name, "archive", () => Number(userStateRow("host")?.archived) === 1);
    await waitFor(() => Number(userStateRow("host")?.archived) === 1, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the archive is not stored" });
    const main = await waitScreen(
      (screen) => findTag(screen, "chat_list_archived_row") && !findText(screen, ROOMS.host.name) && screen,
      "the Archived row and no host row",
    );
    note(`\`${await screenshot(scenarioById("S55"), "main")}\``);

    // The message starts the worker, which then gets the room event that the core kept.
    await hookPost("host", "/post text");
    const event = await waitEvent(
      archived,
      (entry) => entry.via === "room_events" && entry.room === "host" && entry.kind === "UserStateChanged" && entry.data?.archived === true,
      "UserStateChanged on archive",
    );
    await delay(2_000);
    const after = await showChatList();
    expect(!findText(after, ROOMS.host.name), "the host row came back to the main list after /post text");
    expect(Number(userStateRow("host")?.archived) === 1, "the room is no longer archived after /post text");

    tapText(main, "Archived");
    const section = await waitScreen(
      (screen) => findTag(screen, "chat_list_archived_header") && findText(screen, ROOMS.host.name) && screen,
      "the archived section with the host row",
    );
    expect(!findText(section, ROOMS.source.name), "the archived section shows the source room");
    note(`\`${await screenshot(scenarioById("S55"), "archived")}\``);

    const unarchived = eventMark();
    await swipeRow(ROOMS.host.name, "archive", () => Number(userStateRow("host")?.archived ?? 0) === 0);
    await waitFor(() => Number(userStateRow("host")?.archived ?? 0) === 0, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unarchive is not stored" });
    await waitScreen(
      (screen) => !findTag(screen, "chat_list_archived_header") && !findTag(screen, "chat_list_archived_row") && findText(screen, ROOMS.host.name) && screen,
      "the host row back in the main list",
    );
    note(`\`${await screenshot(scenarioById("S55"), "restored")}\``);
    await hookPost("host", "/post text");
    await waitEvent(
      unarchived,
      (entry) => entry.via === "room_events" && entry.room === "host" && entry.kind === "UserStateChanged" && entry.data?.archived === false,
      "UserStateChanged on unarchive",
    );
    note(
      `a swipe archives the room: UserStateChanged ${JSON.stringify(event.data)}, the main list shows the Archived row and not the room, the room stays archived after /post text, the archived section shows it, a swipe there unarchives it and it is back in the main list; delete is not driven (the sample has no delete_room command, S5)`,
    );
  },

  // S56: a worker that starts again first hears which rooms the user muted
  // or archived.
  async S56({ note }) {
    await resetUserState("host");
    await resetUserState("source");
    await showChatList();
    await swipeRow(ROOMS.host.name, "mute", muteSheetShown);
    await tapWhenShown("Always", "the mute durations");
    await waitFor(() => userStateRow("host")?.mutedUntil, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the mute is not stored" });
    await swipeRow(ROOMS.source.name, "archive", () => Number(userStateRow("source")?.archived) === 1);
    await waitFor(() => Number(userStateRow("source")?.archived) === 1, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the archive is not stored" });
    try {
      const lastBoot = Math.max(0, ...events().map((entry) => entry.boot ?? 0));
      shell(serial, ["am", "force-stop", packageName]);
      amStart(serial, ["-n", launcherActivity]);
      await waitForProcess(serial, packageName);
      // The app starts no worker for a product that has rooms, so the launcher opens a room to start it.
      currentRoom = undefined;
      await openRoom("host");
      const snapshot = await waitFor(
        () => {
          const fresh = events().filter((entry) => entry.boot > lastBoot && entry.via === "room_events");
          const host = fresh.find((entry) => entry.room === "host" && entry.kind === "UserStateChanged");
          const source = fresh.find((entry) => entry.room === "source" && entry.kind === "UserStateChanged");
          return host && source ? { fresh, host, source } : undefined;
        },
        { timeoutMs: START_TIMEOUT_MS, intervalMs: 2_000, message: "the new worker got no UserStateChanged snapshot for both rooms" },
      );
      const boot = snapshot.host.boot;
      const firstOthers = events().filter((entry) => entry.boot === boot && entry.via === "room_events" && entry.kind !== "UserStateChanged");
      const snapshotSeq = Math.max(snapshot.host.seq, snapshot.source.seq);
      expect(firstOthers.every((entry) => entry.seq > snapshotSeq), "a room event came before the snapshot");
      expect(snapshot.host.data?.archived === false && snapshot.host.data?.mutedUntil != null, `host snapshot ${JSON.stringify(snapshot.host.data)}`);
      expect(snapshot.source.data?.archived === true && snapshot.source.data?.mutedUntil == null, `source snapshot ${JSON.stringify(snapshot.source.data)}`);
      note(
        `after an app restart, worker boot ${boot} got UserStateChanged host ${JSON.stringify(snapshot.host.data)} (seq ${snapshot.host.seq}) and source ${JSON.stringify(snapshot.source.data)} (seq ${snapshot.source.seq}) as its first room events`,
      );
    } finally {
      await waitProductStarted().catch(() => undefined);
      await showChatList();
      await swipeRow(ROOMS.host.name, "mute", () => userStateRow("host")?.mutedUntil == null);
      await waitFor(() => userStateRow("host")?.mutedUntil == null, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unmute is not stored" });
      const list = await showChatList();
      tapText(list, "Archived");
      await waitScreen((screen) => findTag(screen, "chat_list_archived_header") && findText(screen, ROOMS.source.name), "the archived section");
      await swipeRow(ROOMS.source.name, "archive", () => Number(userStateRow("source")?.archived ?? 0) === 0);
      await waitFor(() => Number(userStateRow("source")?.archived ?? 0) === 0, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unarchive is not stored" });
    }
  },

  // S63: the list search finds the product rooms by name and the host-store
  // text, but never asks a product, so no worker starts.
  async S63({ note }) {
    await showChatList();
    await waitFor(async () => (await workerPages()) === 0, {
      timeoutMs: 120_000,
      intervalMs: 3_000,
      message: "the worker page still runs 120 s after the rooms closed",
    });
    const mark = eventMark();
    const logged = logMark();
    const byName = await searchList("Sample");
    expect(findText(byName, ROOMS.host.name) && findText(byName, ROOMS.source.name), `the search "Sample" shows ${screenSummary(byName)}`);
    note(`\`${await screenshot(scenarioById("S63"), "name")}\``);
    const byText = await searchList("crème");
    expect(byText.some((node) => /Crème brûlée/.test(node.text)), `the search "crème" shows ${screenSummary(byText)}`);
    note(`\`${await screenshot(scenarioById("S63"), "text")}\``);
    const sourceText = await searchList("History message 00050");
    const leaked = sourceText.find((node) => node.text.startsWith("History message 00050") && node.className !== "android.widget.EditText");
    expect(!leaked, `a message of the product-source room is a result: "${leaked?.text}"`);
    await delay(5_000);
    const pages = await workerPages();
    const started = logLines(logged).filter((line) => /TrUAPI worker for .* is running/.test(line.message));
    expect(pages === 0 && started.length === 0 && eventsSince(mark).length === 0, `the list search started the worker: ${pages} page(s), ${started.length} start line(s), ${eventsSince(mark).length} event(s)`);
    inputKeyEvent(serial, KEY.back);
    note(
      'the list search shows both product rooms for "Sample", the host-store message for "crème", and no result for "History message 00050", which only the product of the source room keeps; no worker page, no worker start and no event log line during the search',
    );
  },

  // S69: the user sends a message and leaves the room at once. The wake of the message (W1) holds the worker, so the
  // product still answers; then the worker stops; a message to the stopped worker wakes it again.
  async S69({ note }) {
    await openRoom("host");
    await command("host", "/echo", { reply: true });
    try {
      const text = `w1 closed at once ${String(Date.now()).slice(-5)}`;
      const logged = logMark();
      await typeAndSend(text);
      await showChatList();
      const left = Date.now();
      const woken = await waitForLog(
        logged,
        (line) => line.message.startsWith(`TrUAPI ${productHost} woken for message-in-flight host until `),
        "the wake of the message",
      );
      const echo = await waitFor(
        () => messages("host").find((message) => message.text.startsWith("echo MessagePosted") && message.text.includes(text)),
        { timeoutMs: COMMAND_TIMEOUT_MS, message: `no echo of "${text}" stored` },
      );
      note(`\`${await screenshot(scenarioById("S69"), "list")}\``);
      note(`"${text}" sent and the room left at once: logcat "${woken.message}", the product stored "${echo.text}" ${Math.round((Date.now() - left) / 1000)} s after the room closed`);

      await waitFor(async () => (await workerPages()) === 0, {
        timeoutMs: 120_000,
        intervalMs: 2_000,
        message: "the worker page still runs 120 s after the room closed",
      });
      const stoppedAfter = Math.round((Date.now() - left) / 1000);
      expect(stoppedAfter >= 25, `the worker stopped ${stoppedAfter} s after the room closed, before the wake of 30 s ended`);
      note(`the worker page was gone ${stoppedAfter} s after the room closed (the wake holds 30 s after the message went)`);

      const stopped = `w1 to a stopped worker ${String(Date.now()).slice(-5)}`;
      const asked = logMark();
      hook([
        ["--es", "product_id", productHost],
        ["--es", "room_id", "host"],
        ["--es", "message", stopped],
      ]);
      await waitForLog(asked, (line) => line.message.startsWith(`TrUAPI ${productHost} woken for message-in-flight host until `), "the wake of the message to the stopped worker");
      const second = await waitFor(
        () => messages("host").find((message) => message.text.startsWith("echo MessagePosted") && message.text.includes(stopped)),
        { timeoutMs: ANSWER_TIMEOUT_MS * 2, message: `no echo of "${stopped}" stored` },
      );
      note(`a message to the stopped worker woke it, and the product stored "${second.text}"`);
    } finally {
      await command("host", "/echo", { reply: true });
    }
  },

  // S72: the room declares calls; the header draws the buttons that it declares, and a press sends CallRequested.
  async S72({ note }) {
    await openRoom("host");
    try {
      await command("host", "/features calls both", { reply: true });
      const both = await waitScreen(
        (screen) => findTag(screen, CALL_TAGS.audio) && findTag(screen, CALL_TAGS.video) && screen,
        "both call buttons in the header",
      );
      note(`\`${await screenshot(scenarioById("S72"), "buttons")}\``);
      const mark = eventMark();
      const video = findTag(both, CALL_TAGS.video);
      inputTap(serial, video.center.x, video.center.y);
      const requested = await waitEvent(
        mark,
        (entry) => isCallEvent(entry, "CallRequested") && entry.data?.video === true,
        "room_events CallRequested video=true",
      );
      await waitFor(() => messages("host").some((message) => message.text === "call requested video=true"), {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: 'no "call requested video=true" stored',
      });
      await waitScreen((screen) => findTag(screen, CALL_TAGS.bar) && screen, "the call bar of the call that the product starts");
      note(`a press on the video button: CallRequested ${JSON.stringify(requested.data)} (seq ${requested.seq}); the product answered Active, and the bar shows`);

      await command("host", "/call end", { reply: true });
      await waitScreen((screen) => !findTag(screen, CALL_TAGS.bar) && screen, "no call bar");
      await command("host", "/features calls audio", { reply: true });
      await waitScreen((screen) => findTag(screen, CALL_TAGS.audio) && !findTag(screen, CALL_TAGS.video) && screen, "the audio button only");
      await command("host", "/features calls off", { reply: true });
      await waitScreen((screen) => !findTag(screen, CALL_TAGS.audio) && !findTag(screen, CALL_TAGS.video) && screen, "no call button");
      note(`\`${await screenshot(scenarioById("S72"), "none")}\``);
      note("`/features calls audio` shows the audio button only, `/features calls off` none; a build without call_buttons shows none (ProductRoomCallControllerTest), and this build lists call_buttons (S49)");
    } finally {
      await command("host", "/call end", { reply: true });
      await command("host", "/features calls off", { reply: true });
    }
  },

  // S73: a ringing call rings in the room, in the chat list, in an in-app banner and, with the app in the background,
  // in a CallStyle notification. Accept and Decline answer it; a call that nobody answers stops after 60 s.
  async S73({ note }) {
    await openRoom("host");
    try {
      await command("host", "/call ring", { reply: true });
      const room = await waitScreen((screen) => findTag(screen, CALL_TAGS.ringing) && screen, "the ringing banner of the room");
      const caller = rowTexts(room, CALL_TAGS.ringing);
      expect(caller.includes("Incoming call"), `the banner shows ${JSON.stringify(caller)}`);
      note(`\`${await screenshot(scenarioById("S73"), "ringing")}\``);
      note(`/call ring: the room shows the banner ${JSON.stringify(caller)} with Accept and Decline`);

      // The chat list: the row rings, and the in-app banner rings for the room.
      await showChatList();
      const ringAgain = logMark();
      await hookCommand("host", "/call ring video");
      const list = await waitScreen(
        (screen) => rowNodes(screen, ROOMS.host.name).some((node) => /Incoming video call$/.test(node.text)) && findTag(screen, CALL_TAGS.inApp) && screen,
        'the "Incoming video call" row and the in-app banner',
      );
      expect(findTag(list, CALL_TAGS.inApp), "no in-app banner");
      await waitForLog(ringAgain, (line) => line.message.startsWith(`product call alert ${ROOMS.host.chat.toLowerCase()} banner `), "the banner alert in logcat");
      note(`\`${await screenshot(scenarioById("S73"), "list")}\``);
      note('/call ring video from the list: the row shows "Incoming video call" and the in-app banner shows');

      // The app in the background: a CallStyle notification.
      const background = logMark();
      inputKeyEvent(serial, KEY.home);
      await waitForLog(background, (line) => line.message.startsWith(`product call alert ${ROOMS.host.chat.toLowerCase()} notification `), "the notification alert in logcat");
      const posted = await waitFor(() => callNotification(), { timeoutMs: COMMAND_TIMEOUT_MS, message: "no product call notification in dumpsys" });
      note(`\`${await screenshot(scenarioById("S73"), "background")}\``);
      note(`with the app in the background: a CallStyle notification (${posted})`);
      const answered = await answerFromNotification();
      note(answered);
      amStart(serial, ["-n", launcherActivity]);
      await waitFor(() => !callNotification() && true, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the notification stays with the app in the front" });

      // Accept and Decline in the room.
      await openRoom("host");
      await command("host", "/call end", { reply: true });
      await command("host", "/call ring", { reply: true });
      let screen = await waitScreen((nodes) => findTag(nodes, CALL_TAGS.accept) && nodes, "Accept");
      let mark = eventMark();
      tapTag(screen, CALL_TAGS.accept);
      const accept = await waitEvent(mark, (entry) => isCallEvent(entry, "CallAnswered") && entry.data?.accept === true, "CallAnswered accept=true");
      await waitScreen((nodes) => findTag(nodes, CALL_TAGS.bar) && !findTag(nodes, CALL_TAGS.ringing) && nodes, "the call bar after Accept");
      await command("host", "/call end", { reply: true });
      await command("host", "/call ring", { reply: true });
      screen = await waitScreen((nodes) => findTag(nodes, CALL_TAGS.decline) && nodes, "Decline");
      mark = eventMark();
      tapTag(screen, CALL_TAGS.decline);
      const decline = await waitEvent(mark, (entry) => isCallEvent(entry, "CallAnswered") && entry.data?.accept === false, "CallAnswered accept=false");
      await waitScreen((nodes) => !findTag(nodes, CALL_TAGS.ringing) && nodes, "no banner after Decline");
      note(`Accept sent CallAnswered ${JSON.stringify(accept.data)} and the product showed the bar; Decline sent CallAnswered ${JSON.stringify(decline.data)} and the banner went`);

      // Nobody answers: the banner goes after 60 s.
      await command("host", "/call ring", { reply: true });
      await waitScreen((nodes) => findTag(nodes, CALL_TAGS.ringing) && nodes, "the ringing banner");
      const rang = Date.now();
      await waitScreen((nodes) => !findTag(nodes, CALL_TAGS.ringing) && nodes, "no banner after 60 s", 90_000);
      const after = Math.round((Date.now() - rang) / 1000);
      expect(after >= 50, `the banner went after ${after} s`);
      const listAfter = await showChatList();
      expect(!rowNodes(listAfter, ROOMS.host.name).some((node) => /^Incoming/.test(node.text)), "the row still rings");
      note(`a call that nobody answers: the banner went after ${after} s, and the row does not ring`);
    } finally {
      await openRoom("host");
      await command("host", "/call end", { reply: true });
    }
  },

  // S74: an active call without a media session shows the call bar with its timer. A tap on the bar sends CallOpened,
  // End sends CallEnded.
  async S74({ note }) {
    await openRoom("host");
    try {
      const sent = Date.now();
      await command("host", "/call active", { reply: true });
      await waitScreen((screen) => findTag(screen, CALL_TAGS.bar) && screen, "the call bar");
      await delay(2_500);
      // A dump can take seconds on a busy host, so the check reads the timer twice and compares it with the clock.
      const first = await readCallTimer();
      expect(first.seconds >= 2 && first.seconds <= (first.at - sent) / 1000 + 2, `the timer shows ${first.text} ${Math.round((first.at - sent) / 1000)} s after /call active`);
      await delay(2_000);
      const second = await readCallTimer();
      expect(second.seconds > first.seconds, `the timer stays at ${first.text} (then ${second.text})`);
      const screen = second.screen;
      const timer = `${first.text}, then ${second.text}`;
      note(`\`${await screenshot(scenarioById("S74"), "bar")}\``);
      const bar = findTag(screen, CALL_TAGS.bar);
      let mark = eventMark();
      inputTap(serial, bar.bounds.left + 120, bar.center.y);
      const opened = await waitEvent(mark, (entry) => isCallEvent(entry, "CallOpened"), "CallOpened");
      mark = eventMark();
      tapTag(parseUiNodes(uiDump(serial)), CALL_TAGS.end);
      const ended = await waitEvent(mark, (entry) => isCallEvent(entry, "CallEnded"), "CallEnded");
      await waitScreen((nodes) => !findTag(nodes, CALL_TAGS.bar) && nodes, "no call bar after End");
      note(`/call active: the bar shows the timer ${timer}; a tap sent CallOpened ${JSON.stringify(opened.data)}, End sent CallEnded ${JSON.stringify(ended.data)}, and the product cleared the call`);
    } finally {
      await command("host", "/call end", { reply: true });
    }
  },

  // B9: a notification wake of the room, as the receiving layer will start it (O28: the debug hook stands in), to a
  // fresh preview "Woken by <watch>". Two cold wakes (no worker runs) and one warm wake, in the foreground.
  async B9({ note }) {
    await showChatList();
    const times = [];
    for (const watch of ["b9-cold-1", "b9-cold-2"]) {
      await waitFor(async () => (await workerPages()) === 0, {
        timeoutMs: 150_000,
        intervalMs: 2_000,
        message: "the worker page still runs: no cold start",
      });
      times.push([watch, await timedWake(watch)]);
    }
    times.push(["b9-warm", await timedWake("b9-warm")]);
    note(times.map(([watch, ms]) => `${watch} ${ms} ms`).join(", "));
    note("from the wake hook to the stored preview \"Woken by <watch>\" (chat_room_declarations.preview), the app in the foreground on the chat list");
    await command("host", "/preview none", { reply: true });
  },

  async S7({ note }) {
    await openRoom("host");
    const before = messages("host");
    await command("host", "/post all");
    await command("host", "/help");
    const added = await waitFor(
      () => {
        const fresh = addedMessages("host", before).filter((message) => message.text !== "/post all" && message.text !== "/help");
        return fresh.length >= 8 ? fresh : undefined;
      },
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "the /post all and /help messages are not all stored" },
    );
    const wanted = {
      text: /^Plain text message/,
      rich: /^# Rich text sample/,
      video: /^A sample video/,
      file: /^A sample file/,
      actions: /^Pick one/,
      legacy: /^Legacy message from post_message/,
    };
    const missing = Object.entries(wanted)
      .filter(([, pattern]) => !added.some((message) => pattern.test(message.text)))
      .map(([kind]) => kind);
    expect(!added.some((message) => message.text.startsWith("error ")), `an error reply: ${added.find((message) => message.text.startsWith("error "))?.text}`);
    expect(missing.length === 0, `not stored: ${missing.join(", ")}`);
    expect(added.some((message) => message.type === "CUSTOM"), "no Custom message stored");
    const seen = new Set();
    const shots = [];
    const marks = {
      "Inline sample image": (screen) => screen.some((node) => node.description === "Inline sample image"),
      "Sample video": (screen) => screen.some((node) => node.description === "Sample video"),
      "sample.pdf": (screen) => findText(screen, "sample.pdf"),
      "Pick one / Yes / No": (screen) => findText(screen, "Pick one") && findText(screen, "Yes") && findText(screen, "No"),
      "Sample card": (screen) => findText(screen, "Sample card"),
      "Markdown list of /help": (screen) => screen.some((node) => /\/help/.test(node.text) && /\/meta/.test(node.text)),
    };
    await scrollToNewest();
    // The /help message of the sample takes several screens, and the bodies of /post all are above it.
    for (let attempt = 0; attempt < 30 && seen.size < Object.keys(marks).length; attempt += 1) {
      const screen = parseUiNodes(uiDump(serial));
      let fresh = false;
      for (const [name, test] of Object.entries(marks)) {
        if (!seen.has(name) && test(screen)) {
          seen.add(name);
          fresh = true;
        }
      }
      if (fresh) shots.push(await screenshot(scenarioById("S7"), `host-${shots.length + 1}`));
      await scrollOlder(700);
    }
    await scrollToNewest();
    const unseen = Object.keys(marks).filter((name) => !seen.has(name));
    expect(unseen.length === 0, `not drawn: ${unseen.join(", ")}`);
    note(shots.map((shot) => `\`${shot}\``).join(", "));
    note(`stored: ${added.map((message) => `${message.type}`).join(", ")}; drawn: ${[...seen].join(", ")}`);
  },

  async S8({ note }) {
    await openRoom("host");
    const before = messages("host");
    await command("host", "/stream");
    const streamed = await waitFor(
      () =>
        addedMessages("host", before).find(
          (message) => message.participant === "product" && !/^(ok|error|echo) /.test(message.text),
        ),
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "the streamed message is not stored" },
    );
    await scrollToNewest();
    await waitScreen((screen) => findText(screen, STREAM_SENTENCE), "the full streamed sentence in one bubble", COMMAND_TIMEOUT_MS);
    const revisions = revisionCount(streamed.id);
    expect(revisions === 1, `the streamed message has ${revisions} revision rows`);
    const streamedShot = await screenshot(scenarioById("S8"), "stream");
    await command("host", "/post text");
    const id = await newestPlainText("host");
    await command("host", `/edit ${id} Edited by the launcher`, { reply: true });
    await scrollToNewest();
    const screen = await waitScreen(
      (nodes) => findText(nodes, "Edited by the launcher") && !findText(nodes, plainText(id)) && nodes,
      "the edited bubble in place",
    );
    const editedText = findText(screen, "Edited by the launcher");
    const mark = screen.find(
      (node) => /^Edited\b/.test(node.text) && node !== editedText && Math.abs(node.center.y - editedText.center.y) < 150,
    );
    expect(mark, 'the edited bubble shows no "Edited" mark');
    const editRevisions = revisionCount(`ChatExtension:${extensionId}:host/${id}`);
    expect(editRevisions === 1, `${id} has ${editRevisions} revision rows`);
    const stored = messages("host").find((message) => message.id.endsWith(`/${id}`));
    expect(stored?.text === "Edited by the launcher", `chat_messages.searchableContent reads "${stored?.text.slice(0, 40)}" after the edit`);
    note(`\`${streamedShot}\`, \`${await screenshot(scenarioById("S8"), "host")}\``);
    note(
      `/stream: the edits ended in one bubble with the full sentence and ${revisions} revision row; /edit ${id}: the bubble shows "Edited by the launcher" in place with the mark "${mark.text}" and 1 revision row; chat_messages.searchableContent reads "${stored.text}"`,
    );
  },

  async S9({ note }) {
    await openRoom("host");
    await command("host", "/post text");
    const id = await newestPlainText("host");
    const text = plainText(id);
    await command("host", `/delete ${id}`, { reply: true });
    const deleted = await waitFor(
      () => messages("host").find((message) => message.id.endsWith(`/${id}`) && message.type === "DELETED"),
      { timeoutMs: COMMAND_TIMEOUT_MS, message: `${id} is not a tombstone` },
    );
    await scrollToNewest();
    await waitScreen((screen) => !findText(screen, text) && screen, `"${text}" gone`);
    const shot = await screenshot(scenarioById("S9"), "host");
    const list = await showChatList();
    const preview = rowNodes(list, ROOMS.host.name).find((node) => node.text.length > 3 && !/^\d/.test(node.text));
    expect(preview && !text.test(preview.text), `the list row still previews "${preview?.text}"`);
    note(`\`${shot}\`, \`${await screenshot(scenarioById("S9"), "list")}\``);
    note(
      `chat_messages ${deleted.id} is type DELETED; the bubble "Plain text message ${id.slice(-4)}" is gone; the list row previews "${preview.text}"`,
    );
  },

  async S10({ note }) {
    await openRoom("host");
    await command("host", "/post text");
    const id = await newestPlainText("host");
    const text = plainText(id);
    await command("host", `/react ${id} 👍`, { reply: true });
    await waitFor(() => reactionRows(id).some((reaction) => reaction.origintype !== "USER" && reaction.emoji === "👍"), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "no 👍 of the product stored",
    });
    const mark = eventMark();
    await scrollToNewest();
    const menu = await openMessageMenu(text);
    const thumb = menu.find((node) => node.text === "👍");
    expect(thumb, "the quick reactions offer no 👍");
    inputTap(serial, thumb.center.x, thumb.center.y);
    const reacted = await waitEvent(mark, (entry) => entry.kind === "UserReacted", "UserReacted");
    await waitFor(() => reactionRows(id).some((reaction) => reaction.origintype === "USER"), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "no reaction of the user stored",
    });
    const chip = await waitScreen((screen) => reactionChip(screen, text, "👍"), "the 👍 chip");
    const both = await screenshot(scenarioById("S10"), "host");
    const counted = /2/.test(chip.text) || /, 2 reactions/.test(chip.description) || parseUiNodes(uiDump(serial)).some((node) => node.text === "2" && Math.abs(node.center.y - chip.center.y) < 40);
    await command("host", `/unreact ${id} 👍`, { reply: true });
    await waitFor(() => !reactionRows(id).some((reaction) => reaction.origintype !== "USER"), {
      timeoutMs: COMMAND_TIMEOUT_MS,
      message: "the 👍 of the product stayed",
    });
    expect(reactionRows(id).some((reaction) => reaction.origintype === "USER"), "the reaction of the user went with the product's");
    expect(counted, "the 👍 chip does not count 2 while both reacted");
    note(`\`${both}\``);
    note(
      `chat_message_reactions: 👍 of the product (p1) and of the user on ${id}; the chip counted 2; the user's press sent UserReacted ${JSON.stringify(reacted.data)}; /unreact removed only the product's 👍`,
    );
  },

  async S12({ note }) {
    await openRoom("host");
    await command("host", "/echo", { reply: true });
    await command("host", "/post text");
    const id = await newestPlainText("host");
    const target = plainText(id);
    const kinds = [];

    let mark = eventMark();
    const sent = "Hello from the user";
    await typeAndSend(sent);
    const posted = await waitEvent(mark, (entry) => entry.kind === "MessagePosted" && JSON.stringify(entry.data).includes(sent), "MessagePosted");
    expect(posted.data?.messageId, "MessagePosted has no message id");
    kinds.push(`MessagePosted { message_id: ${posted.data.messageId} }`);

    mark = eventMark();
    await scrollToNewest();
    let menu = await openMessageMenu(target);
    tapText(menu, "Reply");
    await typeAndSend("A reply from the user");
    const reply = await waitEvent(mark, (entry) => entry.kind === "MessagePosted" && JSON.stringify(entry.data).includes("A reply from the user"), "MessagePosted of the reply");
    expect(reply.data?.replyTo, `the reply has no replyTo: ${JSON.stringify(reply.data)}`);
    kinds.push(`MessagePosted { reply_to: ${reply.data.replyTo} }`);

    mark = eventMark();
    menu = await openMessageMenu(sent);
    tapText(menu, "Edit");
    await delay(500);
    await typeAndSend("Hello, edited by the user");
    const edited = await waitEvent(mark, (entry) => entry.kind === "UserEdited", "UserEdited");
    kinds.push(`UserEdited ${JSON.stringify(edited.data).slice(0, 80)}`);

    mark = eventMark();
    await scrollToNewest();
    menu = await openMessageMenu(target);
    const heart = menu.find((node) => node.text === "❤️");
    expect(heart, "the quick reactions offer no ❤️");
    inputTap(serial, heart.center.x, heart.center.y);
    const reacted = await waitEvent(mark, (entry) => entry.kind === "UserReacted", "UserReacted");
    kinds.push(`UserReacted ${JSON.stringify(reacted.data)}`);

    mark = eventMark();
    await scrollToNewest();
    menu = await openMessageMenu("A reply from the user");
    tapText(menu, "Delete");
    const confirm = await waitScreen(
      (screen) => screen.filter((node) => node.text === "Delete").at(-1),
      "the delete confirmation",
    );
    inputTap(serial, confirm.center.x, confirm.center.y);
    const deleted = await waitEvent(mark, (entry) => entry.kind === "UserDeleted", "UserDeleted");
    kinds.push(`UserDeleted ${JSON.stringify(deleted.data)}`);

    const echoes = messages("host").filter((message) => message.text.startsWith("echo ")).map((message) => message.text.split(" ")[1]);
    await command("host", "/echo", { reply: true });
    note(`\`${await screenshot(scenarioById("S12"), "host")}\``);
    note(`event log: ${kinds.join("; ")}; echo replies stored for ${[...new Set(echoes)].join(", ")}`);
  },

  async S13({ note }) {
    await openRoom("host");
    await command("host", "/post actions");
    await scrollToNewest();
    const screen = await waitScreen((nodes) => findText(nodes, "Pick one") && findText(nodes, "Yes") && nodes, "the Actions buttons");
    const before = await screenshot(scenarioById("S13"), "buttons");
    const mark = eventMark();
    tapText(screen, "Yes");
    const triggered = await waitEvent(mark, (entry) => entry.kind === "ActionTriggered", "ActionTriggered");
    expect(triggered.data?.actionId === "act-yes", `ActionTriggered names ${triggered.data?.actionId}`);
    const after = await waitScreen(
      (nodes) => findText(nodes, "Picked Yes") && !findText(nodes, "Pick one") && nodes,
      "the edited Actions message",
    );
    const button = after.find((node) => node.text === "Yes");
    expect(!button, "a Yes button is still on screen");
    note(`\`${before}\`, \`${await screenshot(scenarioById("S13"), "host")}\``);
    note(`the Yes press sent ActionTriggered { message_id: ${triggered.data.messageId}, action_id: act-yes }; the product's Edited made it "Picked Yes" with no buttons`);
  },

  async S14({ note }) {
    await openRoom("host");
    await command("host", "/commands", { reply: true });
    const mark = eventMark();
    const before = messages("host");
    await typeAndSend("/help");
    const commandEvent = await waitEvent(mark, (entry) => entry.kind === "Command", "Command");
    await delay(2_000);
    const posted = eventsSince(mark).filter((entry) => entry.kind === "MessagePosted");
    const stored = addedMessages("host", before).filter((message) => message.text === "/help");
    expect(posted.length === 0, "the command also arrived as MessagePosted");
    expect(stored.length === 0, "the command was stored as a message of the user");
    await command("host", "/commands off", { reply: true, asCommand: true });
    const seen = events().filter((entry) => entry.via === "user_action" && entry.kind === "Seen");
    expect(seen.length > 0, "the product-source room reported no Seen");
    note(`\`${await screenshot(scenarioById("S14"), "host")}\``);
    note(
      `after /commands, "/help" arrived as Command ${JSON.stringify(commandEvent.data)} with no MessagePosted and no stored message; report_seen: the source room sent ${seen.length} user_action Seen (last ${JSON.stringify(seen.at(-1).data)})`,
    );
  },

  async S4({ note }) {
    // The source room has the oldest messages, so it is the last row until it is pinned.
    const variants = [
      ["source", "/preview pin", (list) => listOrder(list)[0] === ROOMS.source.name, "the source room moved to the first row"],
      ["source", "/preview unpin", (list) => listOrder(list).at(-1) === ROOMS.source.name, "the source room went back to the last row"],
      ["host", "/preview badge 3", (list) => rowNodes(list, ROOMS.host.name).some((node) => node.text === "3"), 'the host row shows the badge "3"'],
      ["host", "/preview image", () => true, "the host row shows the badge image (screenshot)"],
      ["host", "/preview none", (list) => !rowNodes(list, ROOMS.host.name).some((node) => /^\d+$/.test(node.text)), "the host row shows no badge"],
      ["host", "/preview typing on", (list) => rowNodes(list, ROOMS.host.name).some((node) => node.text.includes("Sample is typing")), 'the host row shows "Sample is typing"'],
      ["host", "/preview typing off", (list) => !rowNodes(list, ROOMS.host.name).some((node) => node.text.includes("Sample is typing")), "the typing line is gone"],
      ["host", "/preview text Shown in the chat list", (list) => rowNodes(list, ROOMS.host.name).some((node) => node.text === "Shown in the chat list"), "the host row shows the text"],
    ];
    const results = [];
    const shots = [];
    const stored = [];
    for (const [room, text, check, expectation] of variants) {
      if (room === "host") {
        await showChatList();
        await hookCommand("host", text);
      } else {
        await command(room, text, { reply: true });
        await showChatList();
      }
      const hex = declarationRow(room)?.p ?? "";
      stored.push(`${room} 0x${hex.length > 24 ? `${hex.slice(0, 24)}… (${hex.length / 2} bytes)` : hex}`);
      const list = await waitScreen((screen) => check(screen) && screen, `${text}: ${expectation}`, 10_000);
      shots.push(await screenshot(scenarioById("S4"), text.replace(/^\/preview /, "").split(" ").slice(0, 2).join("-")));
      results.push(`${text} (${room}): ${expectation}`);
      expect(list, expectation);
    }
    note(shots.map((shot) => `\`${shot}\``).join(", "));
    note(results.join("; "));
    note(`chat_room_declarations.preview after each: ${stored.join(", ")}`);
  },

  async S6({ note }) {
    shell(serial, ["am", "force-stop", packageName]);
    productServer?.close();
    productServer = undefined;
    const mark = logMark();
    try {
      amStart(serial, ["-n", launcherActivity]);
      await waitForProcess(serial, packageName);
      await showChatList();
      // The reset notice of the app can show after the list and cover a row. `waitScreen` closes the notice first.
      const list = await waitScreen(
        (screen) => findTag(screen, "chats_title") && [ROOMS.host.name, ROOMS.source.name].every((name) => findText(screen, name)) && screen,
        "the host and source rows in the chat list",
        30_000,
      );
      const names = [ROOMS.host.name, ROOMS.source.name].filter((name) => findText(list, name));
      expect(names.length === 2, `the list shows ${names.join(", ") || "no product room"}`);
      await delay(5_000);
      const lines = logLines(mark);
      const failed = lines.find((line) => /worker for .* failed|ERR_CONNECTION_REFUSED|net::ERR/.test(line.message));
      const ran = parseEventLog(logcat.lines.slice(mark)).length;
      expect(ran === 0, `the worker ran and logged ${ran} event log lines`);
      note(`\`${await screenshot(scenarioById("S6"), "list")}\``);
      note(
        `with the product server stopped and the app restarted, the list shows "${names.join('" and "')}" from the database; the worker logged no event (${failed ? `logcat "${failed.message.slice(0, 120)}"` : "no worker start in logcat"})`,
      );
    } finally {
      await startServer();
    }
  },
  async S25({ note }) {
    await openRoom("host");
    try {
      await command("host", "/menu extend", { reply: true });
      const text = "A message whose menu the product extends";
      const mark = eventMark();
      await typeAndSend(text);
      const extended = await openMessageMenu(text);
      const query = await waitEvent(
        mark,
        (entry) => entry.via === "message_actions_query" && entry.room === "host" && JSON.stringify(entry.data).includes(jsonText(text)),
        "a message_actions_query of the message",
      );
      const extendedLabels = menuLabels(extended);
      const extendedShot = await screenshot(scenarioById("S25"), "extend");
      expect(["Reply", "Copy", "Edit", "Delete", "Pin"].every((label) => extendedLabels.includes(label)), `ExtendBuiltIn shows ${extendedLabels.join(", ")}`);
      expect(findText(extended, "Pin").center.y > findText(extended, "Delete").center.y, "Pin is not below the built-in entries");
      await closeMenu();

      await command("host", "/menu hide-edit", { reply: true });
      const hidden = menuLabels(await openMessageMenu(text));
      const hiddenShot = await screenshot(scenarioById("S25"), "host");
      await closeMenu();
      expect(!hidden.includes("Edit") && hidden.includes("Pin") && hidden.includes("Reply"), `ExtendBuiltIn with hidden: [Edit] shows ${hidden.join(", ")}`);
      note(`\`${extendedShot}\`, \`${hiddenShot}\``);
      note(
        `/menu extend: the menu of the user's message asked message_actions_query (seq ${query.seq}) and showed ${extendedLabels.join(", ")}, Pin below the built-in entries; /menu hide-edit: ${hidden.join(", ")}`,
      );
    } finally {
      await closeMenu();
      await command("host", "/menu none", { reply: true });
    }
  },

  async S26({ note }) {
    await openRoom("host");
    try {
      await command("host", "/menu custom", { reply: true });
      await command("host", "/post text");
      const id = await newestPlainText("host");
      const text = plainText(id);
      await scrollToNewest();
      const custom = menuLabels(await openMessageMenu(text));
      const customShot = await screenshot(scenarioById("S26"), "custom");
      await closeMenu();
      expect(custom.join(",") === "Pin,Report", `Custom shows ${custom.join(", ")}`);

      await command("host", "/slow 6000", { reply: true });
      const mark = eventMark();
      const node = await findInFeed(text);
      inputLongPress(serial, node.bounds.left + 20, node.center.y);
      const started = Date.now();
      const fallback = await waitScreen(
        (screen) => findText(screen, "Reply") && !findText(screen, "Loading…") && screen,
        "the default menu after the query time-out",
        20_000,
      );
      const waited = Math.round((Date.now() - started) / 1000);
      const fallbackLabels = menuLabels(fallback);
      const fallbackShot = await screenshot(scenarioById("S26"), "host");
      await closeMenu();
      expect(!fallbackLabels.includes("Pin") && !fallbackLabels.includes("Report"), `the late menu shows ${fallbackLabels.join(", ")}`);
      const asked = eventsSince(mark).filter((entry) => entry.via === "message_actions_query");
      expect(asked.length > 0, "the long press asked no message_actions_query");
      note(`\`${customShot}\`, \`${fallbackShot}\``);
      note(
        `/menu custom: the menu of ${id} showed only ${custom.join(", ")}; under /slow 6000 the menu showed "Loading…", then the defaults ${fallbackLabels.join(", ")} after ${waited} s (the 5 s lookup time-out)`,
      );
    } finally {
      await closeMenu();
      await command("host", "/slow 0", { reply: true, timeoutMs: ANSWER_TIMEOUT_MS });
      await command("host", "/menu none", { reply: true });
    }
  },

  async S28({ note }) {
    await openRoom("host");
    try {
      await command("host", "/menu custom", { reply: true });
      await command("host", "/post text");
      const id = await newestPlainText("host");
      await scrollToNewest();
      const menu = await openMessageMenu(plainText(id));
      const mark = eventMark();
      tapText(menu, "Pin");
      const triggered = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "MessageActionTriggered"), "MessageActionTriggered");
      expect(triggered.data?.actionId === "act-pin" && triggered.data?.messageId === id, `MessageActionTriggered ${JSON.stringify(triggered.data)}`);
      const stored = `MessageActionTriggered act-pin on ${id}`;
      await waitFor(() => messages("host").some((message) => message.text === stored), {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: `no "${stored}" stored`,
      });
      await scrollToNewest();
      await waitScreen((screen) => findText(screen, stored), `"${stored}" on screen`);
      note(`\`${await screenshot(scenarioById("S28"), "host")}\``);
      note(`Pin in the menu of ${id} sent MessageActionTriggered ${JSON.stringify(triggered.data)}; the product answered "${stored}" (chat_messages and screen)`);
    } finally {
      await closeMenu();
      await command("host", "/menu none", { reply: true });
    }
  },

  async S27({ note }) {
    await openRoom("host");
    const text = "Long-press this message to open the App";
    try {
      const before = messages("host");
      await command("host", "/deeplink");
      await waitFor(() => addedMessages("host", before).some((message) => message.text === text), {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: `no "${text}" stored`,
      });
      await scrollToNewest();
      const menu = await openMessageMenu(text);
      tapText(menu, "Open the App");
      const sheet = await waitScreen(
        (screen) => findText(screen, "Open this link?") && findText(screen, /^polkadot:\/\//) && screen,
        "the link confirmation",
      );
      const url = findText(sheet, /^polkadot:\/\//).text;
      const confirmShot = await screenshot(scenarioById("S27"), "confirm");
      const mark = logMark();
      tapText(sheet, "Open");
      // The app gives the link to its deeplink handlers first, then to the system.
      const started = await waitForLog(
        mark,
        (line) => /ActivityTaskManager/.test(line.tag) && line.message.includes("dat=polkadot://") && line.message.includes(productHost),
        "the start of the product link",
      );
      await delay(1_500);
      const after = parseUiNodes(uiDump(serial));
      const shot = await screenshot(scenarioById("S27"), "host");
      note(`\`${confirmShot}\`, \`${shot}\``);
      note(
        `"Open the App" asked "Open this link?" for ${url}; Open handed the link on (logcat "${started.message.slice(0, 90)}…"); ${findTag(after, "chat_message_input") ? "no app page took it" : "a page took it"}: the deeplink handlers of the app open only names of the emulator's network, and the sample product is a .dot name, so the system got it and has no handler`,
      );
    } finally {
      await closeMenu();
      await openRoom("host");
      await command("host", "/menu none", { reply: true });
    }
  },

  async S29({ note }) {
    await openRoom("host");
    try {
      await command("host", "/toolbar 4", { reply: true });
      const bar = await waitScreen(
        (screen) => findDesc(screen, "Button 1") && findDesc(screen, "Button 2") && findDesc(screen, "More actions") && screen,
        "two toolbar buttons and More actions",
      );
      expect(!findDesc(bar, "Button 3") && !findText(bar, "Button 3"), "Button 3 is inline");
      const barShot = await screenshot(scenarioById("S29"), "bar");
      let mark = eventMark();
      tapNode(findDesc(bar, "Button 2"));
      const inline = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "ToolbarActionTriggered"), "ToolbarActionTriggered of Button 2");
      expect(inline.data?.actionId === "tb-2", `Button 2 sent ${JSON.stringify(inline.data)}`);
      tapNode(findDesc(parseUiNodes(uiDump(serial)), "More actions"));
      const overflow = await waitScreen((screen) => findText(screen, "Button 3") && findText(screen, "Button 4") && screen, "Button 3 and 4 in the overflow");
      const overflowShot = await screenshot(scenarioById("S29"), "overflow");
      mark = eventMark();
      tapText(overflow, "Button 3");
      const more = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "ToolbarActionTriggered"), "ToolbarActionTriggered of Button 3");
      expect(more.data?.actionId === "tb-3", `Button 3 sent ${JSON.stringify(more.data)}`);
      note(`\`${barShot}\`, \`${overflowShot}\`, \`${await screenshot(scenarioById("S29"), "host")}\``);
      note(
        `/toolbar 4: Button 1 and Button 2 inline, Button 3 and Button 4 in More actions; Button 2 sent ToolbarActionTriggered tb-2, Button 3 from the overflow sent tb-3`,
      );
    } finally {
      await closeMenu();
      await command("host", "/toolbar 0", { reply: true });
    }
  },

  async S33({ note }) {
    await openRoom("host");
    await command("host", "/footer input", { reply: true });
    const input = await waitScreen(
      (screen) => findText(screen, "Message the sample") && findTag(screen, "chat_message_input") && screen,
      'the hint "Message the sample"',
    );
    expect(!findDesc(input, "Attach"), "the Input footer without attachments shows Attach");
    const inputShot = await screenshot(scenarioById("S33"), "input");
    await command("host", "/footer photo", { reply: true });
    const photo = await waitScreen((screen) => findDesc(screen, "Attach") && screen, "the Attach button");
    expect(!findText(photo, "Message the sample"), "the hint of the last footer stayed");
    note(`\`${inputShot}\`, \`${await screenshot(scenarioById("S33"), "host")}\``);
    note(
      '/footer input: the input shows the hint "Message the sample" and no Attach button; /footer photo (photo, video, file): the Attach button shows, with the default hint',
    );
  },

  async S36({ note }) {
    await openRoom("host");
    try {
      await waitScreen((screen) => findDesc(screen, "Attach") && screen, "the Attach button of /footer photo");
      const before = messages("host");
      const mark = eventMark();
      await attachFromDownloads(SMALL_ATTACHMENT);
      const posted = await waitEvent(
        mark,
        (entry) => isRoomEvent(entry, "host", "MessagePosted") && JSON.stringify(entry.data).includes('"tag":"Image"'),
        "MessagePosted with an image",
      );
      const media = posted.data?.body?.value?.media?.[0];
      const inline = media?.value?.resource;
      expect(inline?.tag === "Inline", `the image is not inline: ${JSON.stringify(inline)?.slice(0, 120)}`);
      const stored = await waitFor(
        () => addedMessages("host", before).find((message) => message.type === "PRODUCT_BODY"),
        { timeoutMs: COMMAND_TIMEOUT_MS, message: "no product body of the user stored" },
      );
      const imageShot = await screenshot(scenarioById("S36"), "image");

      const largeMark = eventMark();
      await attachFromDownloads(LARGE_ATTACHMENT);
      const refusal = await waitScreen(
        (screen) => screen.find((node) => node.text.startsWith("This file is too large to send")),
        "the too-large message",
        15_000,
      );
      const refusedShot = await screenshot(scenarioById("S36"), "host", 0);
      await delay(1_500);
      const large = eventsSince(largeMark).filter((entry) => isRoomEvent(entry, "host", "MessagePosted") && JSON.stringify(entry.data).includes('"tag":"File"'));
      expect(large.length === 0, "the large file reached the product");
      note(`\`${imageShot}\`, \`${refusedShot}\``);
      note(
        `${SMALL_ATTACHMENT} (${media?.value?.meta?.sizeBytes ?? "?"} bytes, ${media?.value?.meta?.mimeType ?? "?"}, ${media?.value?.meta?.width ?? "?"}×${media?.value?.meta?.height ?? "?"}) went inline: MessagePosted RichText with an Inline Image and its metadata, chat_messages ${stored.type}; ${LARGE_ATTACHMENT} (2 MiB) was refused with "${refusal.text}" and did not reach the product`,
      );
    } finally {
      await command("host", "/footer input", { reply: true });
    }
  },

  async S34({ note }) {
    await openRoom("host");
    try {
      await command("host", "/footer empty", { reply: true });
      await waitScreen((screen) => !findTag(screen, "chat_message_input") && findText(screen, "ok /footer empty") && screen, "the room without the input");
      note(`\`${await screenshot(scenarioById("S34"), "host")}\``);
      note('/footer empty: the input and the Attach button went; the feed shows "ok /footer empty"');
    } finally {
      await hookCommand("host", "/footer input");
    }
  },

  async S35({ note }) {
    await openRoom("host");
    try {
      const mark = eventMark();
      await command("host", "/footer custom", { reply: true });
      const footer = await waitScreen(
        (screen) => findText(screen, "The sample product draws this footer") && findText(screen, "Ping") && screen,
        "the footer of the product",
      );
      expect(!findTag(footer, "chat_message_input"), "the custom footer shows the input too");
      const render = await waitEvent(mark, (entry) => entry.via === "render" && entry.kind === "ChatFooter", "a render of ChatFooter");
      const drawnShot = await screenshot(scenarioById("S35"), "drawn");
      const pressed = eventMark();
      tapText(footer, "Ping");
      const action = await waitEvent(pressed, (entry) => entry.via === "renderer_action" && entry.kind === "ChatFooter", "a renderer_action of ChatFooter");
      await waitFor(() => messages("host").some((message) => message.text === "footer ping"), {
        timeoutMs: COMMAND_TIMEOUT_MS,
        message: 'no "footer ping" stored',
      });
      note(`\`${drawnShot}\`, \`${await screenshot(scenarioById("S35"), "host")}\``);
      note(
        `/footer custom: render ChatFooter (seq ${render.seq}) drew "The sample product draws this footer" and Ping in place of the input; Ping sent renderer_action ${JSON.stringify(action.data).slice(0, 100)}; the product posted "footer ping"`,
      );
    } finally {
      await hookCommand("host", "/footer input");
    }
  },

  async S37({ note }) {
    await openRoom("host");
    try {
      await command("host", "/placeholder Nothing here yet", { reply: true });
      await waitFor(() => roomRow("host-empty"), { timeoutMs: COMMAND_TIMEOUT_MS, message: "no host-empty room" });
      await openRoom("host-empty");
      const empty = await waitScreen((screen) => findText(screen, "Nothing here yet") && screen, 'the placeholder "Nothing here yet"');
      expect(!findText(empty, "No messages yet"), "the default placeholder shows too");
      note(`\`${await screenshot(scenarioById("S37"), "host")}\``);
      note('/placeholder Nothing here yet: the empty host-store room "Sample: empty" shows "Nothing here yet" in place of "No messages yet"');
    } finally {
      await openRoom("host");
      await command("host", "/placeholder", { reply: true });
    }
  },

  async S38({ note }) {
    await openRoom("host");
    try {
      await command("host", "/suggest", { reply: true });
      await waitScreen((screen) => findText(screen, "Say hello") && findText(screen, "Show help") && screen, "the two chips");
      const chipsShot = await screenshot(scenarioById("S38"), "chips");
      await typeOnly("typing");
      const typing = parseUiNodes(uiDump(serial));
      expect(!findText(typing, "Say hello") && !findText(typing, "Show help"), "the chips stay while the user types");
      await clearInput();
      let screen = await waitScreen((nodes) => findText(nodes, "Show help") && nodes, "the chips after the input cleared");
      let mark = eventMark();
      tapText(screen, "Show help");
      const selected = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "SuggestionSelected"), "SuggestionSelected");
      expect(selected.data?.suggestionId === "sg-event", `SuggestionSelected ${JSON.stringify(selected.data)}`);
      const sentAsEvent = eventsSince(mark).some((entry) => isRoomEvent(entry, "host", "MessagePosted") && JSON.stringify(entry.data).includes("Show help"));
      expect(!sentAsEvent, "Show help was also sent as a message");
      screen = await waitScreen((nodes) => findText(nodes, "Say hello") && nodes, "the Say hello chip");
      mark = eventMark();
      tapText(screen, "Say hello");
      const posted = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "MessagePosted") && JSON.stringify(entry.data).includes("Say hello"), 'MessagePosted "Say hello"');
      note(`\`${chipsShot}\`, \`${await screenshot(scenarioById("S38"), "host")}\``);
      note(
        `/suggest: chips "Say hello" and "Show help"; both hid while the input held text; Show help sent SuggestionSelected sg-event and no message; Say hello sent MessagePosted "Say hello" (${posted.data?.messageId})`,
      );
    } finally {
      await clearInput().catch(() => {});
      await command("host", "/suggest off", { reply: true });
    }
  },

  async S39({ note }) {
    await openRoom("host");
    try {
      await command("host", "/commands", { reply: true });
      await typeOnly("/h");
      const catalogue = await waitScreen((screen) => findText(screen, "/help") && findText(screen, /^\/history/) && screen, "the / catalogue");
      // The feed shows earlier commands too; the catalogue entries are the ones that start with the typed "/h".
      const entries = catalogue.filter((node) => /^\/h/.test(node.text) && node.tag !== "chat_message_input").map((node) => node.text);
      const catalogueShot = await screenshot(scenarioById("S39"), "catalogue");
      tapText(catalogue, "/help");
      await waitScreen((screen) => findTag(screen, "chat_message_input")?.text.trim() === "/help" && screen, 'the input "/help"');
      const mark = eventMark();
      const send = findTag(parseUiNodes(uiDump(serial)), "chat_send_button");
      tapNode(send);
      const commandEvent = await waitEvent(mark, (entry) => isRoomEvent(entry, "host", "Command"), "Command");
      expect(commandEvent.data?.command === "help", `Command ${JSON.stringify(commandEvent.data)}`);
      await delay(1_500);
      expect(!eventsSince(mark).some((entry) => isRoomEvent(entry, "host", "MessagePosted")), "the command was also posted as a message");
      note(`\`${catalogueShot}\`, \`${await screenshot(scenarioById("S39"), "host")}\``);
      note(`/commands, then "/h" in the input: the catalogue offered ${entries.join(", ")}; a tap on /help put "/help" in the input, and the send arrived as Command ${JSON.stringify(commandEvent.data)} with no MessagePosted`);
    } finally {
      await clearInput().catch(() => {});
      await command("host", "/commands off", { reply: true, asCommand: true });
    }
  },

  async S40({ note }) {
    await openRoom("host");
    try {
      await command("host", "/activity on", { reply: true });
      await waitScreen((screen) => findText(screen, "Sample is typing"), 'the activity line "Sample is typing"');
      const lineShot = await screenshot(scenarioById("S40"), "line");
      await command("host", "/activity off", { reply: true });
      await waitScreen((screen) => !findText(screen, "Sample is typing") && screen, "no activity line");
      await command("host", "/activity on", { reply: true });
      await waitScreen((screen) => findText(screen, "Sample is typing"), "the activity line again");
      const closed = eventMark();
      await showChatList();
      await waitEvent(closed, (entry) => isRoomEvent(entry, "host", "RoomClosed"), "RoomClosed");
      const opened = eventMark();
      await openRoom("host");
      await waitEvent(opened, (entry) => isRoomEvent(entry, "host", "RoomOpened"), "RoomOpened");
      await waitScreen((screen) => findText(screen, "Sample is typing"), "the line that the product restores on RoomOpened");
      note(`\`${lineShot}\`, \`${await screenshot(scenarioById("S40"), "host")}\``);
      note(
        '/activity on showed "Sample is typing" above the input and /activity off cleared it; with the line on, leaving the room sent RoomClosed, and on RoomOpened the product set the line again',
      );
    } finally {
      await command("host", "/activity off", { reply: true });
    }
  },

  async S41({ note }) {
    await openRoom("host");
    try {
      await command("host", "/allowed 👍❤️", { reply: true });
      await scrollToNewest();
      const menu = await openMessageMenu("ok /allowed 👍❤️");
      const picker = quickReactions(menu);
      const more = findDesc(menu, "Show more reactions");
      const shot = await screenshot(scenarioById("S41"), "host");
      await closeMenu();
      expect(picker.join("") === "👍❤️", `the picker offers ${picker.join(" ")}`);
      expect(!more, "the picker offers more reactions");
      await command("host", "/allowed", { reply: true });
      await scrollToNewest();
      const full = quickReactions(await openMessageMenu("ok /allowed"));
      await closeMenu();
      expect(full.length > 2, `after /allowed the picker offers ${full.join(" ")}`);
      note(`\`${shot}\``);
      note(`/allowed 👍❤️: the picker of the menu offered only ${picker.join(" ")} and no "Show more reactions"; after /allowed it offered ${full.join(" ")} again`);
    } finally {
      await closeMenu();
    }
  },

  async S47({ note }) {
    await openRoom("host");
    const opened = eventMark();
    try {
      const before = generation("host");
      await command("host", "/menu extend", { reply: true });
      await waitFor(() => generation("host") > before, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the configuration generation did not change" });
      const extended = generation("host");
      await scrollToNewest();
      const withPin = menuLabels(await openMessageMenu("ok /menu extend"));
      const shot = await screenshot(scenarioById("S47"), "host");
      await closeMenu();
      expect(withPin.includes("Pin"), `after /menu extend the menu shows ${withPin.join(", ")}`);
      await command("host", "/menu none", { reply: true });
      await waitFor(() => generation("host") > extended, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the configuration generation did not change again" });
      await scrollToNewest();
      const withoutPin = menuLabels(await openMessageMenu("ok /menu none"));
      await closeMenu();
      expect(!withoutPin.includes("Pin"), `after /menu none the menu shows ${withoutPin.join(", ")}`);
      const closed = eventsSince(opened).filter((entry) => isRoomEvent(entry, "host", "RoomClosed"));
      expect(closed.length === 0, "the room closed while the policy changed");
      note(`\`${shot}\``);
      note(
        `with the host room open, chat_room_declarations.configGeneration went ${before} → ${extended} → ${generation("host")} on /menu extend and /menu none; the open room took each policy without a reopen (no RoomClosed): menu ${withPin.join(", ")}, then ${withoutPin.join(", ")}. The sample product cannot move a room to the other store, so the source swap of a product_source change stays with ChatRoomSourceContextTest`,
      );
    } finally {
      await closeMenu();
    }
  },

  async S30({ note }) {
    await openRoom("source");
    const mark = eventMark();
    await command("source", "/participants 6", { reply: true });
    const header = await waitScreen((screen) => findText(screen, "10 participants") && screen, 'the header "10 participants"');
    const count = await waitEvent(
      mark,
      (entry) => entry.via === "participants_query" && entry.kind === "Count",
      "a participants_query Count after /participants 6",
    );
    const headerShot = await screenshot(scenarioById("S30"), "header");
    // Every fifth history message comes from p1 to p4, whose names the host asks with `ByIds`.
    const names = ["Person #1", "0x0202…0202", "Contact 0x0303…0303", "truapi-playground.dot"];
    let authors = [];
    await scrollToNewest();
    for (let attempt = 0; attempt < 10 && authors.length === 0; attempt += 1) {
      const screen = parseUiNodes(uiDump(serial));
      authors = names.filter((name) => findText(screen, name));
      if (authors.length === 0) await scrollOlder(700);
    }
    expect(authors.length > 0, "no author name of a participant in the feed");
    const byIds = events().filter((entry) => entry.via === "participants_query" && entry.kind === "ByIds" && entry.room === "source");
    expect(byIds.length > 0, "no participants_query ByIds");
    await scrollToNewest();
    tapNode(findDesc(header, "Participants"));
    const list = await waitScreen(
      (screen) => findText(screen, "Participants") && findText(screen, "Person #1") && findText(screen, "Guest 5") && screen,
      "the participants list",
    );
    const rows = ["Person #1", "0x0202…0202", "Contact 0x0303…0303", "truapi-playground.dot", ...[5, 6, 7, 8, 9, 10].map((n) => `Guest ${n}`)]
      .filter((name) => findText(list, name));
    await waitEvent(mark, (entry) => entry.via === "participants_query" && entry.kind === "Page", "a participants_query Page");
    const pages = eventsSince(mark).filter((entry) => entry.via === "participants_query" && entry.kind === "Page");
    const kinds = ["Person", "Account", "Contact", "App", "Guest"].filter((kind) => findText(list, kind));
    note(`\`${headerShot}\`, \`${await screenshot(scenarioById("S30"), "source")}\``);
    note(
      `/participants 6: participants_query Count (seq ${count.seq}) and the open header "10 participants" without a reopen; authors in the feed from ${byIds.length} participants_query ByIds: ${authors.join(", ")}; the list asked ${pages.length} page(s) and shows ${rows.length} rows (${rows.join(", ")}), with the kinds ${kinds.join(", ")}`,
    );
    inputKeyEvent(serial, KEY.back);
    await delay(800);
  },

  async S31({ note }) {
    await openRoom("source");
    const header = await waitScreen((screen) => findDesc(screen, "Participants") && screen, "the Participants button");
    tapNode(findDesc(header, "Participants"));
    const list = await waitScreen((screen) => findText(screen, "truapi-playground.dot") && screen, "p4 in the participants list");
    let mark = eventMark();
    tapText(list, "truapi-playground.dot");
    const sheet = await waitScreen((screen) => findText(screen, "Mention") && findText(screen, "Open app") && screen, "the sheet of p4");
    const menuQuery = await waitEvent(mark, (entry) => entry.via === "participant_actions_query", "a participant_actions_query");
    const entries = ["Copy address", "Open app", "Mention"].filter((label) => findText(sheet, label));
    const sheetShot = await screenshot(scenarioById("S31"), "sheet");
    mark = eventMark();
    tapText(sheet, "Mention");
    const triggered = await waitEvent(mark, (entry) => isRoomEvent(entry, "source", "ParticipantActionTriggered"), "ParticipantActionTriggered");
    expect(triggered.data?.participantId === "p4" && triggered.data?.actionId === "act-mention", `ParticipantActionTriggered ${JSON.stringify(triggered.data)}`);
    await scrollToNewest();
    await waitScreen((screen) => findText(screen, "ParticipantActionTriggered act-mention on p4"), "the answer of the product");
    note(`\`${sheetShot}\`, \`${await screenshot(scenarioById("S31"), "source")}\``);
    note(
      `p4 (App kind) in the list opened its sheet after participant_actions_query (seq ${menuQuery.seq}): ${entries.join(", ")}; Mention sent ParticipantActionTriggered ${JSON.stringify(triggered.data)}, and the product answered "ParticipantActionTriggered act-mention on p4"`,
    );
  },

  async S32({ note }) {
    await openRoom("source");
    try {
      await command("source", "/reactions on", { reply: true });
      await command("source", "/post text");
      const id = await newestPlainText("source");
      await command("source", `/react ${id} 👍`, { reply: true });
      let mark = eventMark();
      const chip = await waitForChip(plainText(id), "👍");
      inputLongPress(serial, chip.center.x, chip.center.y);
      const dialog = await waitScreen((screen) => findText(screen, "Reactions") && findText(screen, "Person #1") && screen, "who reacted");
      const query = await waitEvent(mark, (entry) => entry.via === "reactions_query", "a reactions_query");
      const answer = await waitAnswer(mark, query);
      const listShot = await screenshot(scenarioById("S32"), "list");
      inputKeyEvent(serial, KEY.back);
      await delay(800);
      expect(answer.kind === "Reactions", `reactions_query answered ${answer.kind}`);

      await command("source", "/reactions na", { reply: true });
      mark = eventMark();
      const again = await waitForChip(plainText(id), "👍");
      inputLongPress(serial, again.center.x, again.center.y);
      const refused = await waitEvent(mark, (entry) => entry.via === "answer" && entry.kind === "NotAvailable", "a NotAvailable answer");
      await delay(2_500);
      const after = parseUiNodes(uiDump(serial));
      expect(!findText(after, "Reactions"), "who reacted showed after NotAvailable");
      const hidden = eventMark();
      const third = await waitForChip(plainText(id), "👍");
      inputLongPress(serial, third.center.x, third.center.y);
      await delay(2_500);
      const asked = eventsSince(hidden).filter((entry) => entry.via === "reactions_query");
      expect(asked.length === 0, "the hidden affordance asked again");
      note(`\`${listShot}\`, \`${await screenshot(scenarioById("S32"), "source")}\``);
      note(
        `/reactions on: a long press on the 👍 chip of ${id} asked reactions_query (seq ${query.seq}), answered Reactions, and "Reactions" listed Person #1; /reactions na: the query was answered NotAvailable (seq ${refused.re}), no list showed, and the next long press asked nothing`,
      );
    } finally {
      await command("source", "/reactions off", { reply: true });
    }
  },
};

const STREAM_SENTENCE =
  "The sample product streams this answer one word at a time so that the host " +
  "can show how one bubble grows in place while the product edits it twenty " +
  "times a second and marks it as sent at the end";

// --------------------------------------------------------------- driving

function step(name) {
  console.log(`==> ${name}`);
}

function expect(condition, message) {
  if (!condition) throw new Error(message);
}

function scenarioById(id) {
  return ROWS.find((scenario) => scenario.id === id);
}

async function startServer() {
  productServer = await startProductServer(productUrl, resolve(productRoot, "out"), productName);
}

/** The system enables a keyboard again when none is enabled, so the launcher does this before each room. */
function disableKeyboards() {
  for (const ime of IMES) shell(serial, ["ime", "disable", ime]);
}

/** Close the on-screen keyboard when it shows, so that it does not cover the feed. */
function hideKeyboard() {
  const state = shell(serial, ["dumpsys", "input_method"]).stdout;
  if (/mInputShown=true/.test(state)) inputKeyEvent(serial, KEY.back);
}

function hook(extras) {
  const result = sendBroadcast(serial, { action: DEBUG_ACTION, component: receiverComponent, extras });
  if (result.error) {
    throw new Error(`The debug broadcast was rejected: ${result.error}; check that ${receiverComponent} is in the APK`);
  }
}

/** Send a command as the user through the debug hook, without the room screen. */
async function hookCommand(room, text) {
  const mark = eventMark();
  const logged = logMark();
  hook([
    ["--es", "product_id", productHost],
    ["--es", "room_id", room],
    ["--es", "message", text],
  ]);
  await waitForLog(logged, (line) => line.tag === E2E_TAG && line.message === `message sent product=${productHost} room=${room}`, "the hook message");
  await waitEvent(mark, (entry) => entry.kind === "MessagePosted" && JSON.stringify(entry.data).includes(jsonText(text)), `MessagePosted "${text}"`);
  await waitReply(room, text, COMMAND_TIMEOUT_MS);
}

/**
 * Type a command into the open room, wait until the product receives it,
 * and, when `reply` is set, until the product answers `ok <command>`.
 */
async function command(room, text, { reply = false, timeoutMs = COMMAND_TIMEOUT_MS, asCommand = false } = {}) {
  await openRoom(room);
  const mark = eventMark();
  await typeAndSend(text);
  await waitEvent(
    mark,
    (entry) =>
      entry.room === room &&
      ((entry.via === "action_subscribe" && (entry.kind === "MessagePosted" || entry.kind === "Command")) ||
        (entry.via === "user_action" && entry.kind === "NewMessage")) &&
      (asCommand || entry.kind === "Command" || JSON.stringify(entry.data).includes(jsonText(text))),
    `the product receives "${text}"`,
    timeoutMs,
  );
  if (reply) await waitReply(room, text, timeoutMs);
}

/** Wait for the `ok <command>` reply; fail on an `error <command>` reply. */
async function waitReply(room, text, timeoutMs) {
  const ok = `ok ${text}`;
  const error = `error ${text}`;
  let polls = 0;
  const answer = await waitFor(
    async () => {
      if (room === "host") {
        return messages("host").find((message) => message.text === ok || message.text.startsWith(error))?.text;
      }
      // The product keeps the messages of the source room, so the reply is read from the screen, at the newest message.
      polls += 1;
      if (polls % 4 === 0) await scrollToNewest();
      return parseUiNodes(uiDump(serial)).find((node) => node.text === ok || node.text.startsWith(error))?.text;
    },
    { timeoutMs, intervalMs: 500, message: `no reply "${ok}"` },
  );
  expect(answer === ok, `the product answered "${answer}"`);
}

async function typeAndSend(text) {
  let typed;
  for (let attempt = 1; attempt <= 3 && !typed; attempt += 1) {
    const screen = await waitScreen((nodes) => findTag(nodes, "chat_message_input") && nodes, "the message input");
    const input = findTag(screen, "chat_message_input");
    inputTap(serial, input.center.x, input.center.y);
    inputKeyCombination(serial, KEY.ctrlLeft, KEY.a);
    inputKeyEvent(serial, KEY.del);
    // The app can lose key events that `input text` sends fast, so the text goes through the clipboard.
    const mark = logMark();
    hook([["--es", "clipboard", text]]);
    await waitForLog(mark, (line) => line.tag === E2E_TAG && line.message === "clipboard set", "the clipboard hook");
    inputKeyEvent(serial, KEY.paste);
    typed = await waitScreen(
      (nodes) => findTag(nodes, "chat_message_input")?.text === text && findTag(nodes, "chat_send_button") && nodes,
      `"${text}" in the input`,
      5_000,
    ).catch(() => undefined);
  }
  expect(typed, `could not type "${text}" into the input`);
  const send = findTag(typed, "chat_send_button");
  inputTap(serial, send.center.x, send.center.y);
  return waitScreen((nodes) => findTag(nodes, "chat_message_input")?.text !== text && nodes, "the input to clear");
}

async function openRoom(room) {
  if (currentRoom === room) {
    const screen = parseUiNodes(uiDump(serial));
    if (findTag(screen, "chat_message_input")) return;
  }
  disableKeyboards();
  amStart(serial, ["-a", "android.intent.action.VIEW", "-d", `polkadotapp://chat?chatId=${ROOMS[room].chat}`]);
  currentRoom = room;
  await waitScreen(
    (screen) =>
      findTag(screen, "chat_message_input") &&
      (findText(screen, ROOMS[room].name) || findText(screen, roomRow(room)?.name ?? ROOMS[room].name)),
    `the ${room} room screen`,
  );
}

/**
 * Wait until the worker logs its preimage line, which ends its start. At its
 * first start the product asks to submit a preimage, and the launcher
 * answers "Don't allow".
 */
async function waitProductStarted() {
  if (reuseApp) return;
  let last = [];
  await waitFor(
    () => {
      last = parseUiNodes(uiDump(serial));
      dismissPrompts(last);
      return events().some((entry) => entry.via === "preimage");
    },
    {
      timeoutMs: START_TIMEOUT_MS,
      intervalMs: 1_000,
      message: () => {
        screencap(serial, resolve(outDir, "start-fail.png"));
        return `the product did not finish its start (no preimage line); \`start-fail.png\` shows ${screenSummary(last)}`;
      },
    },
  );
}

/** Leave the room for the chat list and open it again; gives the event mark before the reopen. */
async function reopenRoom(room) {
  await showChatList();
  const mark = eventMark();
  await openRoom(room);
  return mark;
}

async function showChatList() {
  currentRoom = undefined;
  for (let attempt = 0; attempt < 5; attempt += 1) {
    const screen = parseUiNodes(uiDump(serial));
    dismissPrompts(screen);
    if (findTag(screen, "chats_title")) return screen;
    // An open sheet of the list, such as the mute durations, hides the list from the dump: Back closes it.
    if (findTag(screen, "chat_message_input") || findText(screen, "Mute notifications")) inputKeyEvent(serial, KEY.back);
    else amStart(serial, ["-n", launcherActivity]);
    await delay(1_500);
  }
  return waitScreen((screen) => findTag(screen, "chats_title") && screen, "the chat list");
}

/**
 * Close the sheets that the app shows over the room with a tap: the preimage prompt and the reset notice. The
 * notifications prompt of `/notify` gets "Allow always": a "Don't allow" there is a stored refusal of the product.
 */
function dismissPrompts(screen) {
  const allow = findText(screen, "Allow always");
  if (allow && screen.some((node) => / would like to send notifications$/.test(node.text))) {
    inputTap(serial, allow.center.x, allow.center.y);
    return true;
  }
  for (const label of ["Don't allow", "Dismiss"]) {
    const node = findText(screen, label);
    if (node) {
      inputTap(serial, node.center.x, node.center.y);
      return true;
    }
  }
  return false;
}

async function waitScreen(predicate, what, timeoutMs = COMMAND_TIMEOUT_MS) {
  let last = [];
  return waitFor(
    () => {
      last = parseUiNodes(uiDump(serial));
      if (dismissPrompts(last)) return undefined;
      return predicate(last);
    },
    {
      timeoutMs,
      intervalMs: 500,
      message: () => `no ${what} on screen; it shows ${screenSummary(last)}`,
    },
  );
}

function screenSummary(screen) {
  const texts = screen.map((node) => node.text || node.description).filter(Boolean);
  return JSON.stringify(texts.slice(0, 25).join(" | ").slice(0, 400));
}

function findText(screen, text) {
  return screen.find((node) => (text instanceof RegExp ? text.test(node.text) : node.text === text));
}

function findTag(screen, tag) {
  return screen.find((node) => node.tag === tag);
}

function tapText(screen, text) {
  const node = findText(screen, text);
  expect(node, `no "${text}" on screen; it shows ${screenSummary(screen)}`);
  inputTap(serial, node.center.x, node.center.y);
}

/** The entries of a message menu: the built-in ones, then the ones of the sample product. */
const MENU_LABELS = ["Reply", "Copy", "Edit", "Retry", "Delete", "Pin", "Report", "Open the App"];

/** The emoji of the quick reaction bar of a message menu. */
const QUICK_EMOJI = ["👍", "❤️", "😂", "😮", "😢", "🙏", "🔥", "👏"];

/** Long-press the bubble that shows `text`, and give the screen with its menu once the product answered. */
async function openMessageMenu(text) {
  const node = await findInFeed(text);
  inputLongPress(serial, node.bounds.left + 20, node.center.y);
  return waitScreen(
    (nodes) => MENU_LABELS.some((label) => findText(nodes, label)) && !findText(nodes, "Loading…") && nodes,
    `the menu of "${text}"`,
  );
}

function menuLabels(screen) {
  return MENU_LABELS.filter((label) => findText(screen, label));
}

/** The emoji of the quick reaction bar, left first. */
function quickReactions(screen) {
  return screen
    .filter((node) => QUICK_EMOJI.includes(node.text))
    .sort((a, b) => a.center.x - b.center.x)
    .map((node) => node.text);
}

/** Close a menu, a sheet or a dialog over the room; the room screen and the chat list stay. */
async function closeMenu() {
  for (let attempt = 0; attempt < 3; attempt += 1) {
    const screen = parseUiNodes(uiDump(serial));
    if (findTag(screen, "chat_message_input") || findTag(screen, "chats_title") || findText(screen, "ok /footer empty")) return;
    if (!MENU_LABELS.some((label) => findText(screen, label)) && !findDesc(screen, "Close sheet") && !findText(screen, /^Button \d$/) && !findText(screen, "Reactions")) return;
    inputKeyEvent(serial, KEY.back);
    await delay(800);
  }
}

function findDesc(screen, description) {
  return screen.find((node) => node.description === description);
}

function tapNode(node) {
  expect(node, "no node to tap");
  inputTap(serial, node.center.x, node.center.y);
}

/** Put `text` into the input without a send. */
async function typeOnly(text) {
  const screen = await waitScreen((nodes) => findTag(nodes, "chat_message_input") && nodes, "the message input");
  tapNode(findTag(screen, "chat_message_input"));
  inputKeyCombination(serial, KEY.ctrlLeft, KEY.a);
  inputKeyEvent(serial, KEY.del);
  const mark = logMark();
  hook([["--es", "clipboard", text]]);
  await waitForLog(mark, (line) => line.tag === E2E_TAG && line.message === "clipboard set", "the clipboard hook");
  inputKeyEvent(serial, KEY.paste);
  await waitScreen((nodes) => findTag(nodes, "chat_message_input")?.text === text && nodes, `"${text}" in the input`, 5_000);
}

async function clearInput() {
  const screen = parseUiNodes(uiDump(serial));
  const input = findTag(screen, "chat_message_input");
  if (!input) return;
  tapNode(input);
  inputKeyCombination(serial, KEY.ctrlLeft, KEY.a);
  inputKeyEvent(serial, KEY.del);
  await delay(800);
}

/** The chip of `emoji` under the bubble that shows `text`, found from the newest message up. */
async function waitForChip(text, emoji) {
  await scrollToNewest();
  return waitFor(
    async () => {
      const bubble = await findInFeed(text);
      const screen = parseUiNodes(uiDump(serial));
      return screen.find(
        (node) =>
          node.description.startsWith(`${emoji},`) &&
          node.center.y > bubble.center.y &&
          node.center.y < bubble.bounds.bottom + 220,
      );
    },
    { timeoutMs: COMMAND_TIMEOUT_MS, intervalMs: 1_000, message: `no ${emoji} chip under the message` },
  );
}

/**
 * Attach a file that the launcher put into the Download folder of the
 * device: Attach, the document picker of the system, then the file.
 */
async function attachFromDownloads(fileName) {
  const room = await waitScreen((nodes) => findDesc(nodes, "Attach") && nodes, "the Attach button");
  tapNode(findDesc(room, "Attach"));
  const chooser = await waitScreen(
    (nodes) => (findText(nodes, "Photos & videos") || findText(nodes, "Files")) && nodes,
    "the attachment chooser",
    15_000,
  );
  tapNode(findText(chooser, "Photos & videos") ?? findText(chooser, "Files"));
  let picker = await waitScreen(
    (nodes) => (findText(nodes, fileName) || findDesc(nodes, "Show roots")) && nodes,
    "the document picker",
    15_000,
  );
  if (!findText(picker, fileName)) {
    tapNode(findDesc(picker, "Show roots"));
    const roots = await waitScreen((nodes) => findText(nodes, "Downloads") && nodes, "the Downloads root", 10_000);
    tapNode(findText(roots, "Downloads"));
    picker = await waitScreen((nodes) => findText(nodes, fileName) && nodes, `${fileName} in Downloads`, 10_000);
  }
  tapNode(findText(picker, fileName));
}

function generation(room) {
  return Number(sql(`SELECT configGeneration AS g FROM chat_room_declarations WHERE chatId = X'${ROOMS[room].chat}'`)[0]?.g ?? 0);
}

/** The id-numbered text of a "Plain text message" of the product. */
function plainText(id) {
  return new RegExp(`^Plain text message ${id.slice(-4)}\\b`);
}

/**
 * Find the bubble that shows `text`: first on the screen, then from the
 * newest message up. Gives its node.
 */
async function findInFeed(text, maxScrolls = 15) {
  for (let attempt = 0; attempt <= maxScrolls + 1; attempt += 1) {
    const screen = parseUiNodes(uiDump(serial));
    dismissPrompts(screen);
    const node = findText(screen, text);
    if (node && node.center.y > 380 && node.center.y < 2050) return node;
    if (attempt === 0) await scrollToNewest();
    else await scrollOlder(500);
  }
  throw new Error(`no "${text}" in the feed`);
}

/** A reaction chip with `emoji` in the bubble that shows `text`, or undefined. */
function reactionChip(screen, text, emoji) {
  const bubble = findText(screen, text);
  if (!bubble) return undefined;
  // A chip reads "<emoji>, <n> reactions" for TalkBack, so its text sits in the content description.
  return screen.find(
    (node) =>
      (node.text.includes(emoji) || node.description.startsWith(`${emoji},`)) &&
      node.center.y > bubble.center.y &&
      node.center.y < bubble.bounds.bottom + 200,
  );
}

/** The quote of `target` in the bubble that shows `reply`, when both are on screen. */
function replyQuote(screen, reply, target) {
  const bubble = findText(screen, reply);
  if (!bubble) return undefined;
  const node = screen.find(
    (candidate) =>
      (candidate.text === target || candidate.text === "Earlier message") &&
      candidate.center.y < bubble.center.y &&
      bubble.center.y - candidate.center.y < 260,
  );
  return node ? { node, reply, target } : undefined;
}

async function scrollOlder(distance = 700, durationMs = 400) {
  const top = 700;
  inputSwipe(serial, { x: 540, y: top }, { x: 540, y: top + distance }, durationMs);
  await delay(700);
}

async function scrollNewer(distance = 700, durationMs = 400) {
  const bottom = 1700;
  inputSwipe(serial, { x: 540, y: bottom }, { x: 540, y: bottom - distance }, durationMs);
  await delay(700);
}

async function scrollToNewest() {
  hideKeyboard();
  for (let attempt = 0; attempt < 3; attempt += 1) {
    inputSwipe(serial, { x: 540, y: 1700 }, { x: 540, y: 500 }, 60);
    await delay(400);
  }
  await delay(600);
}

/** The names of the rooms of the chat list, top first. */
function listOrder(screen) {
  const names = [ROOMS.host.name, ROOMS.source.name, "Sample"];
  return screen
    .filter((node) => names.includes(node.text))
    .sort((a, b) => a.center.y - b.center.y)
    .map((node) => node.text);
}

/** The nodes of the chat list row of the room named `name`. */
function rowNodes(screen, name) {
  const title = findText(screen, name);
  if (!title) return [];
  return screen.filter(
    (node) => node !== title && node.center.y > title.bounds.top - 30 && node.center.y < title.bounds.bottom + 110,
  );
}

function killWorkerRenderer() {
  const listing = rootShell(serial, "ps -A -o PID,NAME").stdout;
  const renderers = listing
    .split(/\r?\n/)
    .map((line) => line.trim().split(/\s+/))
    .filter(([pid, name]) => /^\d+$/.test(pid ?? "") && /sandboxed_process/.test(name ?? ""));
  expect(renderers.length > 0, "found no WebView renderer process");
  for (const [pid] of renderers) rootShell(serial, `kill -9 ${pid}`);
  return renderers.map(([pid, name]) => `${name.replace(/:org\.chromium.*$/, "")} (pid ${pid})`).join(", ");
}

async function screenshot(scenario, suffix, settleMs = 800) {
  hideKeyboard();
  await delay(settleMs);
  const number = scenario.id.replace(/^S/, "").padStart(2, "0");
  const name = `s${number}-${suffix}.png`;
  screencap(serial, resolve(outDir, name));
  return name;
}

// ------------------------------------- personas, threads, branches, reasoning

/** The personas of the sample product, from `chat_bots`. */
function personaRows() {
  return sql(`SELECT botId, name FROM chat_bots WHERE extensionId = '${extensionId}'`);
}

/** The meta rows of a room, from `chat_message_meta`. */
function metaRows(room) {
  return sql(`SELECT messageId, timestamp FROM chat_message_meta WHERE chatId = X'${ROOMS[room].chat}'`);
}

/** Whether `node` lies inside the bounds of `container`. */
function insideOf(node, container) {
  return (
    node.center.x >= container.bounds.left &&
    node.center.x <= container.bounds.right &&
    node.center.y >= container.bounds.top &&
    node.center.y <= container.bounds.bottom
  );
}

/** Whether the author line `name` shows just above the bubble that shows `text`. */
function authorAbove(screen, text, name) {
  const bubble = findText(screen, text);
  if (!bubble) return false;
  return screen.some((node) => node.text === name && node.center.y < bubble.center.y && bubble.center.y - node.center.y < 220);
}

/**
 * Long-press the newest rich message of the sample, on its inline image (the Markdown text of a rich message is not
 * a node of the screen dump), and give the screen with its menu.
 */
async function openRichMenu() {
  await scrollToNewest();
  let image;
  for (let attempt = 0; attempt < 8 && !image; attempt += 1) {
    image = parseUiNodes(uiDump(serial)).find(
      (node) => node.description === "Inline sample image" && node.center.y > 380 && node.center.y < 2050,
    );
    if (!image) await scrollOlder(500);
  }
  expect(image, "no image of the rich message on screen");
  inputLongPress(serial, image.center.x, image.center.y);
  return waitScreen((nodes) => findText(nodes, "Copy") && nodes, "the menu of the rich message");
}

/** The thread chip under the bubble that shows `text`, or undefined. */
function chipUnder(screen, text) {
  const bubble = findText(screen, text);
  if (!bubble) return undefined;
  return screen.find(
    (node) => node.tag === "chat_thread_chip" && node.center.y > bubble.center.y && node.bounds.top - bubble.bounds.bottom < 260,
  );
}

/** The branch control under the bubble that shows `text`: its position and its arrows, or undefined. */
function branchUnder(screen, text) {
  const bubble = findText(screen, text);
  if (!bubble) return undefined;
  const position = screen
    .filter((node) => node.tag === "chat_branch_position" && node.center.y > bubble.center.y && node.bounds.top - bubble.bounds.bottom < 260)
    .sort((a, b) => a.center.y - b.center.y)[0];
  if (!position) return undefined;
  const near = (tag) => screen.find((node) => node.tag === tag && Math.abs(node.center.y - position.center.y) < 60);
  return { position, previous: near("chat_branch_previous"), next: near("chat_branch_next") };
}

// ----------------------------------------------- mute, archive and search

/** What the user set for a room, from `chat_room_declarations`. */
function userStateRow(room) {
  return sql(`SELECT mutedUntil, archived FROM chat_room_declarations WHERE chatId = X'${ROOMS[room].chat}'`)[0];
}

/**
 * Swipe the chat list row of the room named `name`: to the end mutes or
 * unmutes it, to the start archives or unarchives it.
 */
/**
 * Swipe the row `name` to the end (`mute`) or to the start (`archive`), until `done` is true. An emulator under load
 * can drop a swipe gesture, and the row then does not move. The launcher swipes again, at most three times, and notes
 * each repeat in the report.
 */
async function swipeRow(name, action, done) {
  for (let attempt = 1; ; attempt += 1) {
    const screen = await waitScreen((nodes) => findText(nodes, name) && nodes, `the row "${name}"`);
    const title = findText(screen, name);
    const y = title.center.y + 40;
    // A swipe that starts at an edge of the screen is the system Back gesture, so it starts inside.
    if (action === "mute") inputSwipe(serial, { x: 300, y }, { x: 1000, y }, 300);
    else inputSwipe(serial, { x: 850, y }, { x: 100, y }, 300);
    await delay(1_200);
    const moved = await waitFor(done, { timeoutMs: 6_000, intervalMs: 500, message: "no change" }).then(
      () => true,
      () => false,
    );
    if (moved || attempt === 3) return;
    observations.push(`the ${action} swipe on "${name}" changed nothing; the launcher swiped again (attempt ${attempt + 1})`);
  }
}

/** The mute durations sheet that a swipe to the end opens on a row that is not muted. */
const muteSheetShown = () => findText(parseUiNodes(uiDump(serial)), "Mute notifications");

async function tapWhenShown(label, what) {
  const screen = await waitScreen((nodes) => findText(nodes, label) && nodes, what);
  tapText(screen, label);
  await delay(800);
}

/** Clear the mute and the archive that an earlier scenario left on the room, through the list. */
async function resetUserState(room) {
  const state = userStateRow(room);
  if (!state) return;
  if (Number(state.archived) === 1) {
    const list = await showChatList();
    tapText(list, "Archived");
    await waitScreen((screen) => findTag(screen, "chat_list_archived_header") && findText(screen, ROOMS[room].name), "the archived section");
    await swipeRow(ROOMS[room].name, "archive", () => Number(userStateRow(room)?.archived ?? 0) === 0);
    await waitFor(() => Number(userStateRow(room)?.archived ?? 0) === 0, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unarchive is not stored" });
  }
  if (userStateRow(room)?.mutedUntil != null && Number(userStateRow(room).mutedUntil) > Date.now()) {
    await showChatList();
    await swipeRow(ROOMS[room].name, "mute", () => userStateRow(room)?.mutedUntil == null);
    await waitFor(() => userStateRow(room)?.mutedUntil == null, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unmute is not stored" });
  }
  await showChatList();
}

/** Send a command that posts messages as the user through the debug hook; it has no `ok` reply. */
async function hookPost(room, text) {
  const before = messages(room);
  const mark = eventMark();
  const logged = logMark();
  hook([
    ["--es", "product_id", productHost],
    ["--es", "room_id", room],
    ["--es", "message", text],
  ]);
  await waitForLog(logged, (line) => line.tag === E2E_TAG && line.message === `message sent product=${productHost} room=${room}`, "the hook message");
  await waitEvent(mark, (entry) => entry.kind === "MessagePosted" && JSON.stringify(entry.data).includes(jsonText(text)), `MessagePosted "${text}"`);
  await waitFor(() => addedMessages(room, before).some((message) => /^Plain text message/.test(message.text)), {
    timeoutMs: COMMAND_TIMEOUT_MS,
    message: `no message of the product after "${text}"`,
  });
}

/**
 * Type a `/notify` command into the host room and wait for its `ok` reply. The first one asks the user to allow
 * notifications for the product: the launcher taps "Allow always" here and in `dismissPrompts`.
 */
async function notifyFromRoom(text, { mutedRoom = false } = {}) {
  await openRoom("host");
  const before = messages("host").length;
  const mark = eventMark();
  await typeAndSend(text);
  await waitEvent(mark, (entry) => entry.room === "host" && JSON.stringify(entry.data ?? "").includes(jsonText(text)), `the product receives "${text}"`);
  await waitFor(
    () => {
      const screen = parseUiNodes(uiDump(serial));
      const allow = findText(screen, "Allow always");
      if (allow) inputTap(serial, allow.center.x, allow.center.y);
      const reply = messages("host").slice(before).find((message) => message.text === `ok ${text}` || message.text.startsWith(`error ${text}`));
      if (reply && /Permission denied/.test(reply.text)) {
        throw new Error(
          `the product answered "${reply.text}"${mutedRoom ? " into the muted room" : ""}: the notifications permission of the product is refused before the mute rule runs (logcat truapi.native.callback.device_permission; the core keeps a "Don't allow"), so this host cannot show the suppression`,
        );
      }
      if (reply) expect(reply.text === `ok ${text}`, `the product answered "${reply.text}"`);
      return reply;
    },
    { timeoutMs: COMMAND_TIMEOUT_MS, intervalMs: 700, message: `no reply "ok ${text}"` },
  );
}

/** The number of notifications of the sample product that the system shows. */
function productNotifications() {
  const dump = shell(serial, ["dumpsys", "notification", "--noredact"]).stdout;
  return (dump.match(/Sample notification from host/g) ?? []).length;
}

/** Open the in-room search of the open room and type `query`; gives the screen with its results. */
async function searchInRoom(query) {
  let screen = parseUiNodes(uiDump(serial));
  if (!findTag(screen, "chat_room_search_field")) {
    const action = await waitScreen((nodes) => findTag(nodes, "chat_room_search_action") && nodes, "the search action of the header");
    const button = findTag(action, "chat_room_search_action");
    inputTap(serial, button.center.x, button.center.y);
    screen = await waitScreen((nodes) => findTag(nodes, "chat_room_search_field") && nodes, "the search field");
  }
  await pasteInto(findTag(screen, "chat_room_search_field"), query);
  return waitScreen((nodes) => nodes.some((node) => node.tag === "chat_room_search_result") && nodes, `the results of "${query}"`);
}

/** Open the search of the chat list and type `query`; gives the screen after the results settle. */
async function searchList(query) {
  let screen = parseUiNodes(uiDump(serial));
  let field = screen.find((node) => node.className === "android.widget.EditText");
  if (!field) {
    screen = await showChatList();
    if (!findText(screen, "Search")) {
      // The search entry shows when the list is pulled down.
      inputSwipe(serial, { x: 540, y: 700 }, { x: 540, y: 1500 }, 400);
      screen = await waitScreen((nodes) => findText(nodes, "Search") && nodes, "the search entry of the list");
    }
    tapText(screen, "Search");
    screen = await waitScreen((nodes) => nodes.find((node) => node.className === "android.widget.EditText") && nodes, "the search field of the list");
    field = screen.find((node) => node.className === "android.widget.EditText");
  }
  await pasteInto(field, query);
  await delay(2_500);
  return parseUiNodes(uiDump(serial));
}

/** Replace the text of a field with `text` through the clipboard, as `typeAndSend` does. */
async function pasteInto(field, text) {
  inputTap(serial, field.center.x, field.center.y);
  inputKeyCombination(serial, KEY.ctrlLeft, KEY.a);
  inputKeyEvent(serial, KEY.del);
  const mark = logMark();
  hook([["--es", "clipboard", text]]);
  await waitForLog(mark, (line) => line.tag === E2E_TAG && line.message === "clipboard set", "the clipboard hook");
  inputKeyEvent(serial, KEY.paste);
  await delay(800);
}

// ------------------------------------------------------------- wakes and calls

/** The test tags of the call parts of a product room (`ProductRoomCallTags`). */
const CALL_TAGS = {
  audio: "chat_call_button_audio",
  video: "chat_call_button_video",
  ringing: "chat_call_ringing_banner",
  accept: "chat_call_accept",
  decline: "chat_call_decline",
  bar: "chat_call_bar",
  timer: "chat_call_timer",
  end: "chat_call_end",
  inApp: "chat_call_in_app_banner",
};

function isCallEvent(entry, kind) {
  return entry.via === "room_events" && entry.room === "host" && entry.kind === kind;
}

function tapTag(screen, tag) {
  const node = findTag(screen, tag);
  expect(node, `no ${tag} on screen; it shows ${screenSummary(screen)}`);
  inputTap(serial, node.center.x, node.center.y);
}

/** The texts inside the node tagged `tag`. */
function rowTexts(screen, tag) {
  const box = findTag(screen, tag)?.bounds;
  if (!box) return [];
  return screen
    .filter((node) => node.text && node.center.y >= box.top && node.center.y <= box.bottom && node.center.x >= box.left && node.center.x <= box.right)
    .map((node) => node.text);
}

/** The text of the call timer in seconds, with the screen and the time of the dump. */
async function readCallTimer() {
  const screen = await waitScreen((nodes) => findTag(nodes, CALL_TAGS.timer) && nodes, "the call timer");
  const text = findTag(screen, CALL_TAGS.timer).text;
  const parts = text.split(":").map(Number);
  expect(parts.length >= 2 && parts.every(Number.isFinite), `the timer shows "${text}"`);
  return { text, seconds: parts.reduce((total, part) => total * 60 + part, 0), at: Date.now(), screen };
}

/** The notification of a ringing product room that the system shows, or undefined. */
function callNotification() {
  const dump = shell(serial, ["dumpsys", "notification", "--noredact"]).stdout;
  const key = dump.split(/\r?\n/).find((line) => line.includes(`product-room-call.${ROOMS.host.chat.toLowerCase()}`));
  return key?.trim().slice(0, 160);
}

/**
 * Answer in the notification shade: the product gets CallAnswered and the room opens. The shade of a headless
 * emulator can lay out the buttons of a call notification differently, so this step notes what it saw.
 */
async function answerFromNotification() {
  const mark = eventMark();
  shell(serial, ["cmd", "statusbar", "expand-notifications"]);
  await delay(1_500);
  const shade = parseUiNodes(uiDump(serial));
  // The system labels the answer button of a video call "Video", and wraps the label in direction marks.
  const answer = shade.find((node) => /(^|[^\p{L}])(Answer|Video)[^\p{L}]*$/u.test(node.text) && !/Incoming/.test(node.text));
  if (!answer) {
    shell(serial, ["cmd", "statusbar", "collapse"]);
    return `the shade showed no Answer button (${screenSummary(shade)}); Answer from the notification is not driven`;
  }
  inputTap(serial, answer.center.x, answer.center.y);
  const answered = await waitEvent(mark, (entry) => isCallEvent(entry, "CallAnswered") && entry.data?.accept === true, "CallAnswered from the notification", 45_000);
  await waitScreen((screen) => findTag(screen, CALL_TAGS.bar) && screen, "the room with the call bar after Answer", 45_000);
  currentRoom = "host";
  const shot = await screenshot(scenarioById("S73"), "answered");
  return `\`${shot}\`; Answer in the notification sent CallAnswered ${JSON.stringify(answered.data)} and opened the room with the call bar`;
}

/** Wake the host room through the debug hook, and give the time to the stored preview "Woken by <watch>". */
async function timedWake(watch) {
  const wanted = Buffer.from(`Woken by ${watch}`).toString("hex").toUpperCase();
  const logged = logMark();
  const started = Date.now();
  hook([
    ["--es", "product_id", productHost],
    ["--es", "room_id", "host"],
    ["--es", "wake", watch],
  ]);
  await waitFor(() => previewHex().includes(wanted), {
    timeoutMs: 90_000,
    intervalMs: 150,
    message: `no preview "Woken by ${watch}" stored`,
  });
  const elapsed = Date.now() - started;
  const ack = await waitForLog(logged, (line) => line.tag === E2E_TAG && line.message.startsWith("woken "), "the ack of the wake hook");
  expect(ack.message.includes("held until="), `the wake was not held: ${ack.message}`);
  // The answer to Woken also sends a notification; its first prompt asks for the permission of the product.
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) dismissPrompts(parseUiNodes(uiDump(serial)));
  return elapsed;
}

// ------------------------------------------------------------- evidence

/** The host port of the WebView DevTools socket; a second run on the same host needs another one. */
const DEVTOOLS_PORT = Number(process.env.TRUAPI_ANDROID_E2E_DEVTOOLS_PORT ?? 9339);

/** The `location.origin` of a WebView page, through its DevTools socket. */
async function pageOrigin(socketUrl) {
  if (!socketUrl) return undefined;
  const socket = new WebSocket(socketUrl);
  try {
    return await Promise.race([
      new Promise((resolveOrigin, reject) => {
        socket.onerror = reject;
        socket.onmessage = (message) => resolveOrigin(JSON.parse(message.data).result?.result?.value);
        socket.onopen = () =>
          socket.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression: "location.origin", returnByValue: true } }));
      }),
      delay(5_000).then(() => undefined),
    ]);
  } catch {
    return undefined;
  } finally {
    socket.close();
  }
}

/** The number of WebView pages of the product worker, from the DevTools page list of the app. */
async function workerPages() {
  const pid = shell(serial, ["pidof", packageName]).stdout.trim();
  if (!/^\d+$/.test(pid)) return 0;
  forwardAbstractSocket(serial, DEVTOOLS_PORT, `webview_devtools_remote_${pid}`);
  try {
    const response = await fetch(`http://127.0.0.1:${DEVTOOLS_PORT}/json`);
    const pages = (await response.json()).filter((page) => page.type === "page");
    // The worker page loads its HTML from data, so its URL is `about:blank`; its origin names the product.
    const origins = await Promise.all(pages.map((page) => pageOrigin(page.webSocketDebuggerUrl)));
    return origins.filter((origin) => origin === url.origin).length;
  } catch {
    return 0;
  } finally {
    removeForward(serial, DEVTOOLS_PORT);
  }
}

function events() {
  return parseEventLog(logcat.lines);
}

function eventMark() {
  return events().length;
}

function eventsSince(mark) {
  return events().slice(mark);
}

async function waitEvent(mark, predicate, what, timeoutMs = COMMAND_TIMEOUT_MS) {
  return waitFor(() => eventsSince(mark).find(predicate), {
    timeoutMs,
    intervalMs: 500,
    message: () => `no ${what} in the event log (${eventsSince(mark).slice(-4).map((entry) => `${entry.via}/${entry.kind}`).join(", ") || "nothing new"})`,
  });
}

/** The answer line of the query or user action `request`. */
async function waitAnswer(mark, request, timeoutMs = COMMAND_TIMEOUT_MS) {
  return waitEvent(
    mark,
    (entry) => entry.via === "answer" && entry.re === request.seq && entry.boot === request.boot,
    `the answer to ${request.via} ${request.kind} (seq ${request.seq})`,
    timeoutMs,
  );
}

function isNewMessage(entry, text) {
  return entry.via === "user_action" && entry.kind === "NewMessage" && JSON.stringify(entry.data).includes(jsonText(text));
}

function isRetry(entry) {
  return entry.via === "user_action" && entry.kind === "Retry";
}

function isRoomEvent(entry, room, kind) {
  return entry.via === "action_subscribe" && entry.room === room && entry.kind === kind;
}

function jsonText(text) {
  return JSON.stringify(text).slice(1, -1);
}

function anchorCursor(query) {
  const anchor = query.data?.anchor;
  return anchor?.value?.cursor ?? undefined;
}

function describeAnchor(query) {
  const cursor = anchorCursor(query);
  return `${query.kind} { cursor: ${cursor === undefined ? "None" : JSON.stringify(cursor)} }`;
}

function pageMessages(answer) {
  return answer.data?.messages ?? [];
}

function pageNext(answer) {
  const data = answer.data ?? {};
  return data.next ?? data.nextCursor ?? data.cursor ?? null;
}

/** The answered pages of the source room since `mark`. */
function sourcePages(mark) {
  const recent = eventsSince(mark);
  return recent
    .filter((entry) => entry.via === "messages_query" && entry.room === "source")
    .map((query) => {
      const answer = recent.find((entry) => entry.via === "answer" && entry.re === query.seq && entry.boot === query.boot);
      return answer ? { query, answer, count: pageMessages(answer).length, next: pageNext(answer) } : undefined;
    })
    .filter(Boolean);
}

/** The page since `mark` whose answer has no next cursor, or undefined. */
function finalPage(mark) {
  return sourcePages(mark).find((page) => page.next === null && page.query.kind === "Before");
}

function logMark() {
  return logcat.lines.length;
}

function logLines(mark) {
  return logcat.lines.slice(mark).map(parseLogcatLine).filter(Boolean);
}

async function waitForLog(mark, predicate, what, timeoutMs = COMMAND_TIMEOUT_MS) {
  return waitFor(() => logLines(mark).find(predicate), {
    timeoutMs,
    intervalMs: 500,
    message: `no ${what} in logcat`,
  });
}

function sql(query) {
  return runAsSqliteJson(serial, packageName, DATABASE, query);
}

function roomRow(room) {
  return sql(`SELECT name FROM chat_rooms WHERE id = X'${ROOMS[room].chat}'`)[0];
}

function declarationRow(room) {
  return sql(
    `SELECT hex(config) AS cfg, description, hex(preview) AS p FROM chat_room_declarations WHERE chatId = X'${ROOMS[room].chat}'`,
  )[0];
}

function previewHex() {
  return declarationRow("host")?.p ?? "";
}

/** The messages of a host-store room in feed order. */
function messages(room) {
  return sql(
    `SELECT id, type, status, COALESCE(searchableContent, '') AS text, COALESCE(originParticipantId, '') AS participant, replyToMessageId AS replyTo FROM chat_messages WHERE chatId = X'${ROOMS[room].chat}' ORDER BY sortOrder, timestamp`,
  );
}

/** The messages of a host-store room that are not in `before` (a list of earlier rows). */
function addedMessages(room, before) {
  const known = new Set(before.map((message) => message.id));
  return messages(room).filter((message) => !known.has(message.id));
}

function reactionRows(productMessageId) {
  const id = `ChatExtension:${extensionId}:host/${productMessageId}`;
  return sql(`SELECT emoji, origintype FROM chat_message_reactions WHERE messageId = '${id.replaceAll("'", "''")}'`);
}

function revisionCount(messageId) {
  return Number(sql(`SELECT COUNT(*) AS n FROM message_revisions WHERE messageId = '${messageId.replaceAll("'", "''")}'`)[0]?.n ?? 0);
}

/** The id of the newest "Plain text message" of the product in a room. */
async function newestPlainText(room) {
  if (room === "host") {
    return waitFor(
      () => {
        const plain = messages("host").filter((message) => /^Plain text message \d{4}/.test(message.text));
        return plain.at(-1)?.id.replace(/^.*\//, "");
      },
      { timeoutMs: COMMAND_TIMEOUT_MS, message: "no plain text message stored" },
    );
  }
  await scrollToNewest();
  const node = await waitScreen(
    (screen) => screen.filter((candidate) => /^Plain text message \d{4}/.test(candidate.text)).sort((a, b) => b.center.y - a.center.y)[0],
    "a plain text message",
  );
  return `${room}-p-${/(\d{4})/.exec(node.text)[1]}`;
}

async function main() {
  rmSync(outDir, { recursive: true, force: true });
  mkdirSync(outDir, { recursive: true });


  function cleanup() {
    logcat?.stop();
    productServer?.close();
    productServer = undefined;
    if (serial) {
      removeReversePort(serial, productPort);
      if (bootedEmulator && !keepEmulator) killEmulator(serial);
    }
  }

  for (const [signal, code] of [
    ["SIGINT", 130],
    ["SIGTERM", 143],
  ]) {
    process.once(signal, () => {
      cleanup();
      process.exit(code);
    });
  }

  const rows = [];
  let failure;
  try {
    // The DotNS name goes into the room links and the deeplink of the product.
    const productBuild =
      process.env.TRUAPI_ANDROID_E2E_SKIP_PRODUCT_BUILD !== "1"
        ? runAsync("yarn", ["build"], {
            cwd: productRoot,
            env: { ...process.env, CHAT_V2_DOTNS_NAME: productHost },
          })
        : Promise.resolve();
    productBuild.catch(() => {});

    step("boot the emulator");
    bootedEmulator = !isDeviceOnline(requestedSerial);
    serial = await ensureEmulator({ avd, serial: requestedSerial, log: (line) => console.log(`    ${line}`) });
    console.log(`    device ${serial}, package ${packageName}`);

    step("build the sample product");
    await productBuild;
    if (!existsSync(worker)) throw new Error(`Chat product worker not found after build: ${worker}`);

    step(`serve ${relative(repoRoot, productRoot)}/out at ${productUrl}`);
    await startServer();
    reversePort(serial, productPort);
    // A restart of the adb server drops the port; the logcat stream connects again by itself.
    onAdbServerRestart(() => reversePort(serial, productPort));

    // The launcher pastes text from the clipboard, so it needs no on-screen
    // keyboard. A keyboard covers the feed in the screenshots.
    disableKeyboards();

    clearLogcat(serial);
    logcat = streamLogcat(serial, resolve(outDir, "app.log"));
    if (!reuseApp) {
      step(`install ${relative(repoRoot, apk)}`);
      installApk(serial, apk);
      clearAppData(serial, packageName);
      grantPermission(serial, packageName, "android.permission.POST_NOTIFICATIONS");
      // The Attach button asks for the camera first; S36 picks files from the Download folder.
      grantPermission(serial, packageName, "android.permission.CAMERA");
      pushFile(serial, resolve(productRoot, "assets", SMALL_ATTACHMENT), `/sdcard/Download/${SMALL_ATTACHMENT}`);
      const large = largeAttachment();
      pushFile(serial, large, `/sdcard/Download/${LARGE_ATTACHMENT}`);
      rmSync(large);

      step("start the app, seed the identity and install the product");
      amStart(serial, ["-n", launcherActivity]);
      await waitForProcess(serial, packageName);
      await delay(3_000);
      const seeded = logMark();
      hook([
        ["--ez", "seed_identity", "true"],
        ["--es", "username", username],
        ["--es", "product_id", productHost],
        ["--es", "product_name", productName],
        ["--es", "worker_url", workerUrl],
      ]);
      await waitForLog(seeded, (line) => line.tag === E2E_TAG && line.message === "seed_identity done", "seed_identity done");
      await waitForLog(
        seeded,
        (line) => line.tag === E2E_TAG && line.message === `product registered id=${productHost}`,
        "product registered",
      );

      step("relaunch the app on the seeded identity");
      shell(serial, ["am", "force-stop", packageName]);
    }
    amStart(serial, ["-n", launcherActivity]);
    await waitForProcess(serial, packageName);

    step("wait for the product to create its rooms");
    await waitFor(() => roomRow("host") && roomRow("source"), {
      timeoutMs: START_TIMEOUT_MS,
      intervalMs: 2_000,
      message: `the product did not create its rooms in ${START_TIMEOUT_MS} ms; see app.log`,
    });

    step("wait for the product to finish its start");
    await waitProductStarted();

    for (const scenario of ROWS) {
      if (onlyScenarios.length > 0 && !onlyScenarios.includes(scenario.id)) continue;
      step(`${scenario.id} ${scenario.title}`);
      const result = await runScenario(scenario);
      // S49 runs after the rows that prove the features, so it can check them.
      rows.push(scenario.id === "S49" ? capabilityVerdict(result, rows, listedFeatures) : result);
      console.log(`    ${rows.at(-1).result}: ${rows.at(-1).evidence}`);
    }
  } catch (error) {
    failure = error instanceof Error ? error.message : String(error);
    console.error(`\nthe run stopped: ${error instanceof Error ? error.stack : error}`);
    const remaining = ROWS.filter(
      (scenario) =>
        !rows.some((existing) => existing.id === scenario.id) &&
        (onlyScenarios.length === 0 || onlyScenarios.includes(scenario.id)),
    );
    rows.push(...blockedRows(failure, remaining));
  } finally {
    if (serial) shell(serial, ["am", "force-stop", packageName]);
    cleanup();
  }

  const eventLog = logcat ? parseEventLog(logcat.lines) : [];
  writeFileSync(
    resolve(outDir, "event-log.jsonl"),
    `${eventLog.map((entry) => JSON.stringify(entry)).join("\n")}\n`,
  );
  const report = renderScenarioReport(
    "Truapi Android Chat Extension v2 Diagnosis",
    byScenarioNumber(rows),
  );
  const notes = [
    "",
    `Device: ${serial ?? "none"} (AVD ${avd}). App: \`${packageName}\`. Product: \`${productHost}\`, rooms \`host\` and \`source\`.`,
    `Event log: ${eventLog.length} lines in \`event-log.jsonl\`; the app's logcat in \`app.log\`.`,
    ...(observations.length > 0 ? ["", "Observed while driving the app:", ...observations.map((line) => `- ${line}`)] : []),
    ...(failure ? ["", `**The run stopped:** ${failure}`] : []),
  ];
  const reportPath = resolve(outDir, "diagnosis-report.md");
  writeFileSync(reportPath, `${report}${notes.join("\n")}\n`);

  const expected = ROWS.filter(
    (scenario) => scenario.drive && (onlyScenarios.length === 0 || onlyScenarios.includes(scenario.id)),
  );
  const failures = scenarioFailures(rows, expected);
  console.log(
    JSON.stringify({
      device: serial,
      package: packageName,
      productHost,
      report: relative(repoRoot, reportPath),
      rows: rows.length,
      passed: rows.filter((entry) => entry.result === "pass").length,
      skipped: rows.filter((entry) => entry.result === "skip").length,
      failures,
    }),
  );
  if (failures.length > 0) process.exit(1);
}

await main();
