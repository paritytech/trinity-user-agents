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
import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
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
  ANDROID_CHAT_V2_SCENARIOS,
  MIB,
  blockedRows,
  noisePng,
  stagingBoundSkip,
  byScenarioNumber,
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
};
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
    // A driver gives `{ skip }` when the emulator cannot carry the row; the evidence keeps what it proved.
    const outcome = await driver({ scenario, note });
    if (outcome?.skip) return row(scenario, "skip", Date.now() - started, [outcome.skip, ...evidence].join("; "));
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

/**
 * The rows that show each Chat feature that the app can list (S49). The app
 * lists a feature only when these rows pass on the emulator.
 */
const FEATURE_ROWS = {
  product_source_rooms: "S15 to S24 and S46",
  custom_bodies: "S7",
  mute_archive: "S54 to S56",
  search: "S61 to S63",
  preimage_attachments: "S57 to S60 (S57 with a file under the chain bound, O32)",
};

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
    expect(declarations.host.cfg === "" && declarations.host.p === "", "the host room stores a config or preview");
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
    note(`ReactionPlaced: chip "${reacted.text}"`);
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
    await openRoom("source");
    await command("source", "/fail unavailable");
    const opened = await reopenRoom("source");
    const first = await waitEvent(opened, (entry) => entry.via === "messages_query", "a messages_query");
    const refused = await waitAnswer(opened, first);
    expect(refused.kind === "NotAvailable", `the first answer is ${refused.kind}`);
    const second = await waitEvent(
      opened,
      (entry) => entry.via === "messages_query" && entry.seq > first.seq,
      "the retry of the messages_query",
    );
    const answer = await waitAnswer(opened, second);
    expect(answer.kind === "Page", `the retry was answered ${answer.kind}`);
    expect(describeAnchor(second) === describeAnchor(first), "the retry asked another page");
    await waitScreen((screen) => findText(screen, "ok /fail unavailable"), "the newest page on screen");
    note(`\`${await screenshot(scenarioById("S46"), "source")}\``);
    note(`${describeAnchor(first)} answered NotAvailable; the host asked the same page once more and got ${answer.kind}; the feed shows the newest page`);
  },

  async S22({ note }) {
    await openRoom("source");
    await command("source", "/history 560");
    const opened = await reopenRoom("source");
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
      const header = await waitScreen(
        (screen) => findText(screen, "Renamed by /meta") && screen,
        "the new description in the open header",
      );
      note('the open header shows the new description "Renamed by /meta"');
      note(`\`${await screenshot(scenarioById("S3"), "host")}\``);
      const named = await waitScreen(
        (screen) => findText(screen, "Renamed by the launcher") && screen,
        "the new name in the open header",
        10_000,
      ).catch(() => undefined);
      expect(
        named,
        `the open header still shows "${header.find((node) => node.text === ROOMS.host.name)?.text ?? "?"}"; it reads the room name once (ChatEngine.observeChatDisplay)`,
      );
      await showChatList();
      const list = await waitScreen((screen) => findText(screen, "Renamed by the launcher") && screen, "the new name in the chat list");
      expect(list, "the list row kept the old name");
      note("the list row and the open header show the new name");
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
    // The core gives the staging bound of 8 MiB only when the app lists preimage_attachments.
    const preimageBound = listed.includes("preimage_attachments") ? String(8 * MIB) : "0";
    expect(match[2] === String(MIB) && match[3] === preimageBound, `bounds inline ${match[2]} B, preimage ${match[3]} B`);
    const unbacked = listed.filter((name) => !FEATURE_ROWS[name]);
    expect(unbacked.length === 0, `the app lists ${unbacked.join(", ")} without a row of the launcher that passes for it`);
    await waitScreen((screen) => findText(screen, /^caps: /), "the capabilities message");
    note(`\`${await screenshot(scenarioById("S49"), "host")}\``);
    note(
      `the app lists ${listed.join(", ") || "none"}; the product read ${read.join(", ") || "none"}; inline ${match[2]} B, preimage ${match[3]} B; the rows of the listed names: ${listed.map((name) => `${name} ${FEATURE_ROWS[name]}`).join(", ") || "none"}`,
    );
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
    await swipeRow(ROOMS.host.name, "mute");
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
    await swipeRow(ROOMS.host.name, "mute");
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
    await swipeRow(ROOMS.host.name, "archive");
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
    await swipeRow(ROOMS.host.name, "archive");
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
    await swipeRow(ROOMS.host.name, "mute");
    await tapWhenShown("Always", "the mute durations");
    await waitFor(() => userStateRow("host")?.mutedUntil, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the mute is not stored" });
    await swipeRow(ROOMS.source.name, "archive");
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
      await swipeRow(ROOMS.host.name, "mute");
      await waitFor(() => userStateRow("host")?.mutedUntil == null, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unmute is not stored" });
      const list = await showChatList();
      tapText(list, "Archived");
      await waitScreen((screen) => findTag(screen, "chat_list_archived_header") && findText(screen, ROOMS.source.name), "the archived section");
      await swipeRow(ROOMS.source.name, "archive");
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
    for (let attempt = 0; attempt < 12 && seen.size < Object.keys(marks).length; attempt += 1) {
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
    note(`\`${streamedShot}\`, \`${await screenshot(scenarioById("S8"), "host")}\``);
    note(
      `/stream: the edits ended in one bubble with the full sentence and ${revisions} revision row; /edit ${id}: the bubble shows "Edited by the launcher" in place with the mark "${mark.text}" and 1 revision row; chat_messages.searchableContent still reads "${stored?.text.slice(0, 40)}" (app gap: edits do not update it)`,
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
    const counted = /2/.test(chip.text) || parseUiNodes(uiDump(serial)).some((node) => node.text === "2" && Math.abs(node.center.y - chip.center.y) < 40);
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

  // S60: the first staged attachment of the product shows the public-storage
  // notice. Cancel keeps it unsent; Send stages it; a later attachment of the
  // same product, in the other room, shows no notice.
  async S60({ note }) {
    await prepareStaging(note);
    for (const room of ["host", "source"]) await declareStaging(room, true);
    preimageAnswer = "Allow always";

    await openRoom("host");
    let mark = logMark();
    let notice = await pickFile(stagingFile("s60-photo.png", () => noisePng(1024, 500, RUN_SEED + 60)), { notice: true });
    expect(findText(notice, /^Files over 1 MB sent in .* are stored on the public Bulletin chain/), "the notice names no product");
    note(`\`${await screenshot(scenarioById("S60"), "notice")}\``);
    tapText(notice, "Cancel");
    await delay(3_000);
    expect(outboxEntries().length === 0, `Cancel kept ${outboxEntries().length} outbox entries`);
    expect(!stageLines(mark).length, "Cancel staged the attachment");
    note("Cancel: no outbox entry and no staging");

    mark = logMark();
    notice = await pickFile(stagingFile("s60-photo.png", () => noisePng(1024, 500, RUN_SEED + 60)), { notice: true });
    note("the notice shows again after Cancel");
    tapText(notice, "Send");
    const staged = await waitStage(mark, "the staging after Send");
    note(`Send: ${staged.message}`);
    await deleteOutboxBubbles();

    await openRoom("source");
    mark = logMark();
    await pickFile(stagingFile("s60-file.pdf", () => bytesOf(1.2 * MIB)), { notice: false });
    const second = await waitStage(mark, "the staging in the source room", { failOnNotice: true });
    note(`no notice for the second attachment of the product (source room): ${second.message}`);
    note(`\`${await screenshot(scenarioById("S60"), "source")}\``);
    await deleteOutboxBubbles();
  },

  // S57: a 3 MiB photo and a 6 MiB file. The Bulletin chain of the emulator
  // decides the bound (O32); the path is then proved with files under it.
  async S57({ note }) {
    await prepareStaging(note);
    preimageAnswer = "Allow always";
    const bounds = [];
    for (const [name, make] of [
      ["s57-photo-3mib.png", () => noisePng(1024, 1024, RUN_SEED + 57)],
      ["s57-file-6mib.pdf", () => bytesOf(6 * MIB)],
    ]) {
      await openRoom("host");
      const mark = logMark();
      await pickFile(stagingFile(name, make), { notice: false });
      const line = await waitStage(mark, `the staging of ${name}`);
      note(`${name}: ${line.message.replace(/^truapi\.chat\.stage outbox:\S+ /, "")}`);
      const tooLarge = /TooLarge \(the attachment is larger than (\d+) bytes\)/.exec(line.message);
      if (tooLarge) {
        bounds.push(Number(tooLarge[1]));
        note(`the room says: ${await attachmentBanner(mark, "This file is too large to send. Choose one under 2 MB.", /TooLarge\(maxBytes=2097152\)/, line.banner)}`);
      }
      await deleteOutboxBubbles();
    }

    // Under the bound: a photo in the host-store room and a file in the product-source room.
    const proof = [];
    for (const [room, name, make] of [
      ["host", "s57-photo-under.png", () => noisePng(1024, 600, RUN_SEED + 157)],
      ["source", "s57-file-under.pdf", () => bytesOf(1.9 * MIB)],
    ]) {
      await openRoom(room);
      const mark = logMark();
      const events = eventMark();
      await pickFile(stagingFile(name, make), { notice: false });
      const line = await waitStage(mark, `the staging of ${name}`);
      const outcome = line.message.replace(/^truapi\.chat\.stage outbox:\S+ /, "");
      if (/: Staged$/.test(line.message)) {
        const media = await waitEvent(events, (entry) => entry.via === "media" && entry.room === room, `the media item in ${room}`, ANSWER_TIMEOUT_MS);
        expect(/ Preimage$/.test(media.data.line), `the product got ${media.data.line}`);
        const fetched = await waitEvent(events, (entry) => entry.via === "preimage" && entry.room === room, `the preimage read back in ${room}`, ANSWER_TIMEOUT_MS);
        proof.push(`${room}: ${name} ${outcome}, the product got "${media.data.line}" and read it back: ${fetched.kind} ${fetched.data.size} B`);
      } else {
        proof.push(`${room}: ${name} ${outcome}`);
        await deleteOutboxBubbles();
      }
      note(`\`${await screenshot(scenarioById("S57"), `${room}-under`)}\``);
    }
    note(proof.join("; "));

    // B7: 1 B gives the time of the call without the bytes.
    note(`B7: ${await probeStaging(1)}; ${await probeStaging(8 * MIB)}`);

    const failed = proof.filter((line) => !/Fetched/.test(line));
    const boundSkip = bounds.length > 0 ? stagingBoundSkip(Math.min(...bounds), [3 * MIB, 6 * MIB]) : undefined;
    if (!boundSkip && failed.length === 0) return undefined;
    const allowance = failed.find((line) => /AllowanceUnavailable/.test(line));
    expect(failed.length === 0 || allowance, `the files under the bound did not arrive: ${failed.join("; ")}`);
    const reasons = [
      boundSkip,
      allowance && "the emulator account is not a personhood ring member, so the core has no Bulletin allowance for the product (AllowanceUnavailable) and nothing under the bound can arrive either",
    ].filter(Boolean);
    return { skip: reasons.length > 0 ? reasons.join("; ") : "skip" };
  },

  // S59: a refused submit marks the attachment Failed and keeps its bytes;
  // Retry stages it again; Delete removes the kept bytes.
  async S59({ note }) {
    await prepareStaging(note);
    await openRoom("source");
    await resetPreimagePermission();
    preimageAnswer = "Don't allow";
    let mark = logMark();
    await pickFile(stagingFile("s59-retry.pdf", () => bytesOf(1.2 * MIB)), { notice: false });
    const failed = await waitStage(mark, "the refused staging");
    expect(/PermissionDenied/.test(failed.message), `the first staging gave ${failed.message}`);
    note(`the room says: ${await attachmentBanner(mark, PERMISSION_BANNER, /AttachmentPermissionPresentationError/, failed.banner)}`);
    const entryId = /outbox:\S+/.exec(failed.message)[0];
    expect(outboxEntries().length === 1, `the outbox keeps ${outboxEntries().length} entries`);
    note(`\`${await screenshot(scenarioById("S59"), "failed")}\``);
    note(`refused: ${failed.message}; the bubble is Failed and the outbox keeps the bytes`);

    await resetPreimagePermission();
    preimageAnswer = "Allow always";
    mark = logMark();
    const events = eventMark();
    let menu = await openMessageMenu("s59-retry.pdf");
    expect(findText(menu, "Retry"), `the menu of the failed attachment offers ${menuLabels(menu).join(", ")}`);
    tapText(menu, "Retry");
    const retried = await waitStage(mark, "the staging after Retry", { entryId });
    note(`Retry stages again: ${retried.message}`);
    let blocked;
    if (/: Staged$/.test(retried.message)) {
      const media = await waitEvent(events, (entry) => entry.via === "media" && entry.room === "source", "the media item after Retry", ANSWER_TIMEOUT_MS);
      note(`the product got "${media.data.line}"`);
    } else {
      expect(/AllowanceUnavailable/.test(retried.message), `Retry gave ${retried.message}`);
      blocked = "skip: Retry stages again, but the emulator account is not a personhood ring member, so the core has no Bulletin allowance for the product (AllowanceUnavailable) and the retry cannot succeed";
    }

    await resetPreimagePermission();
    preimageAnswer = "Don't allow";
    mark = logMark();
    await pickFile(stagingFile("s59-delete.pdf", () => bytesOf(1.1 * MIB)), { notice: false });
    await waitStage(mark, "the staging of the attachment to delete");
    const before = outboxEntries().length;
    menu = await openMessageMenu("s59-delete.pdf");
    tapText(menu, "Delete");
    const confirm = await waitScreen((screen) => screen.filter((node) => node.text === "Delete").at(-1), "the delete confirmation");
    inputTap(serial, confirm.center.x, confirm.center.y);
    await waitFor(() => outboxEntries().length === before - 1, { timeoutMs: COMMAND_TIMEOUT_MS, message: "Delete kept the bytes" });
    await waitScreen((screen) => !findText(screen, "s59-delete.pdf") && screen, "the deleted bubble to go");
    note(`Delete removes the bubble and the kept bytes (${before} to ${before - 1} outbox entries)`);
    await deleteOutboxBubbles();
    preimageAnswer = "Don't allow";
    return blocked ? { skip: blocked } : undefined;
  },

  // S58: no upload without the declaration, above 8 MiB, or with
  // PreimageSubmit refused.
  async S58({ note }) {
    await prepareStaging(note);
    await declareStaging("host", false);
    await openRoom("host");
    const kept = outboxEntries().length;
    let mark = logMark();
    let events = eventMark();
    await pickFile(stagingFile("s58-undeclared.pdf", () => bytesOf(1.5 * MIB)), { notice: false });
    note(`without preimage_attachments: ${await attachmentBanner(mark, "This file is too large to send. Choose one under 1 MB.", /attachment exceeds 1MB/)}`);

    await declareStaging("host", true);
    const huge = logMark();
    await pickFile(stagingFile("s58-huge.pdf", () => bytesOf(8 * MIB + 1)), { notice: false });
    note(`above 8 MiB: ${await attachmentBanner(huge, "This file is too large to send. Choose one under 8 MB.", /attachment exceeds 8MB/)}`);
    expect(stageLines(mark).length === 0 && outboxEntries().length === kept, "a refused attachment reached the outbox");

    await resetPreimagePermission();
    preimageAnswer = "Don't allow";
    await pickFile(stagingFile("s58-refused.pdf", () => bytesOf(1.5 * MIB)), { notice: false });
    const refused = await waitStage(mark, "the refused staging");
    expect(/PermissionDenied/.test(refused.message), `the staging gave ${refused.message}`);
    note(`the room says: ${await attachmentBanner(mark, PERMISSION_BANNER, /AttachmentPermissionPresentationError/, refused.banner)}`);
    note(`PreimageSubmit refused: ${refused.message.replace(/^truapi\.chat\.stage outbox:\S+ /, "")}`);
    note(`\`${await screenshot(scenarioById("S58"), "refused")}\``);
    await delay(2_000);
    const uploaded = eventsSince(events).filter((entry) => entry.via === "media");
    expect(uploaded.length === 0, `the product received ${uploaded.length} media item(s)`);
    expect(!stageLines(mark).some((line) => /: Staged$/.test(line.message)), "an attachment was staged");
    note("nothing staged and no media item reached the product");
    await deleteOutboxBubbles();
    for (const room of ["host", "source"]) await declareStaging(room, false);
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
      const list = await showChatList();
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
  return ANDROID_CHAT_V2_SCENARIOS.find((scenario) => scenario.id === id);
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
  const answer = await waitFor(
    () => {
      if (room === "host") {
        return messages("host").find((message) => message.text === ok || message.text.startsWith(error))?.text;
      }
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
  const mark = logMark();
  disableKeyboards();
  amStart(serial, ["-a", "android.intent.action.VIEW", "-d", `polkadotapp://chat?chatId=${ROOMS[room].chat}`]);
  currentRoom = room;
  await waitScreen(
    (screen) =>
      findTag(screen, "chat_message_input") &&
      (findText(screen, ROOMS[room].name) || findText(screen, roomRow(room)?.name ?? ROOMS[room].name)),
    `the ${room} room screen`,
  );
  await waitWorkerSettled(mark);
  await retryUnavailableRoom(room);
}

/**
 * Wait until the worker runs and its client is connected. A room screen
 * that opens soon after another one closed restarts the worker, and the
 * client of the restarted worker connects some time after the app reports that it runs.
 */
async function waitWorkerSettled(mark, timeoutMs = 45_000) {
  let seen = -1;
  let quietSince = Date.now();
  await waitFor(
    () => {
      const lines = logLines(mark);
      const running = lines.filter((line) => /TrUAPI worker for .* is running/.test(line.message));
      const connected = lines.filter((line) => line.message.includes("truapi.ws_bridge.connection_open"));
      const count = running.length + connected.length;
      if (count !== seen) {
        seen = count;
        quietSince = Date.now();
      }
      // The client of a new worker connects just before or after the app reports that the worker runs.
      const lastRunning = running.at(-1);
      const settled =
        !lastRunning ||
        connected.some((line) => logTime(line) >= logTime(lastRunning) - 5_000);
      return settled && Date.now() - quietSince > 2_500;
    },
    { timeoutMs, intervalMs: 500, message: "the worker did not settle: it runs but its client is not connected" },
  );
}

/** The time of a logcat line in milliseconds of its day. */
function logTime(line) {
  const [hours, minutes, seconds] = line.time.split(":").map(Number);
  return ((hours * 60 + minutes) * 60 + seconds) * 1000;
}

/**
 * Wait until the worker logs its preimage line, which ends its start. At its
 * first start the product asks to submit a preimage, and the launcher
 * answers "Don't allow". When the prompt does not reach the screen, the start
 * of the product waits for an answer that never comes. Then the launcher
 * restarts the app once and notes it.
 */
async function waitProductStarted() {
  const started = () => events().some((entry) => entry.via === "preimage");
  const prompted = () =>
    waitFor(
      () => {
        dismissPrompts(parseUiNodes(uiDump(serial)));
        return started();
      },
      { timeoutMs: 60_000, intervalMs: 1_000, message: "the product did not finish its start" },
    );
  if (reuseApp) return;
  try {
    await prompted();
  } catch {
    const request = logLines(0).find((line) => line.message.includes("remote_permission"));
    observations.push(
      `${new Date().toISOString().slice(11, 19)} the product's start waited 60 s on "${request?.message ?? "a permission request"}", whose prompt never reached the screen; the launcher restarted the app`,
    );
    shell(serial, ["am", "force-stop", packageName]);
    amStart(serial, ["-n", launcherActivity]);
    await waitForProcess(serial, packageName);
    await prompted();
  }
}

/** Tap Retry when the room shows that the product does not answer, and note it. */
async function retryUnavailableRoom(room) {
  const screen = parseUiNodes(uiDump(serial));
  const line = screen.find((node) => node.text.startsWith("The app does not answer"));
  if (!line) return;
  observations.push(`${new Date().toISOString().slice(11, 19)} the ${room} room opened as unavailable ("${line.text}"); the launcher tapped Retry`);
  const retry = screen.find((node) => node.text === "Retry" && Math.abs(node.center.y - line.center.y) < 120);
  if (retry) inputTap(serial, retry.center.x, retry.center.y);
  await waitScreen((nodes) => !nodes.some((node) => node.text.startsWith("The app does not answer")) && nodes, "the room after Retry", 45_000);
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
  if (screen.some((node) => PREIMAGE_PROMPT.test(node.text))) {
    const answer = findText(screen, preimageAnswer);
    if (answer) {
      inputTap(serial, answer.center.x, answer.center.y);
      return true;
    }
  }
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

/** Long-press the bubble that shows `text`, and give the screen with its menu. */
async function openMessageMenu(text) {
  const node = await findInFeed(text);
  inputLongPress(serial, node.bounds.left + 20, node.center.y);
  return waitScreen((nodes) => (findText(nodes, "Reply") || findText(nodes, "Copy") || findText(nodes, "Retry")) && nodes, `the menu of "${text}"`);
}

function menuLabels(screen) {
  return ["Reply", "Copy", "Edit", "Retry", "Delete"].filter((label) => findText(screen, label));
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
  return screen.find(
    (node) => node.text.includes(emoji) && node.center.y > bubble.center.y && node.center.y < bubble.bounds.bottom + 200,
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

// ------------------------------------------------- staged attachments (P10.3)

/** How the launcher answers the PreimageSubmit prompt of the product. */
let preimageAnswer = "Don't allow";
const PREIMAGE_PROMPT = / would like to submit preimages$/;
const stagingDir = resolve(tmpdir(), `truapi-android-staging-${process.pid}`);
let stagingServed = false;

/**
 * Turn staging on for the rows of P10.3. A debug build serves
 * preimage_attachments for the launcher also while the app does not list
 * the feature. The camera permission is granted, so the attachment picker
 * opens without a system prompt.
 */
async function prepareStaging(note) {
  grantPermission(serial, packageName, "android.permission.CAMERA");

  const mark = logMark();
  hook([["--ez", "serve_preimage_attachments", "true"]]);
  await waitForLog(mark, (line) => line.tag === E2E_TAG && line.message === "preimage attachments served", "the staging hook");
  if (!stagingServed) {
    const listed = logLines(0).filter((line) => line.message.startsWith("truapi.chat.capabilities ")).at(-1);
    note(listed?.message.includes("preimage_attachments")
      ? "the app lists preimage_attachments"
      : "the app does not list preimage_attachments; the debug hook serves it for this run");
    stagingServed = true;
  }
  // The first start of the product stored "Don't allow" for PreimageSubmit; each row decides again.
  await openRoom("host");
  await resetPreimagePermission();
}

async function declareStaging(room, on) {
  await command(room, "/footer photo", { reply: true });
  await command(room, `/features preimage ${on ? "on" : "off"}`, { reply: true });
}

/** Clear the stored PreimageSubmit decision of the product, as a user does in its settings. The worker must run. */
function resetPreimagePermission() {
  const mark = logMark();
  hook([
    ["--es", "product_id", productHost],
    ["--ez", "reset_preimage_permission", "true"],
  ]);
  return waitForLog(mark, (line) => line.tag === E2E_TAG && /^(preimage permission reset|error preimage_permission)/.test(line.message), "the permission reset")
    .then((line) => expect(line.message === "preimage permission reset", line.message));
}

/** Write a file of the run and copy it into the Download folder of the device. */
function stagingFile(name, make) {
  mkdirSync(stagingDir, { recursive: true });
  const local = resolve(stagingDir, name);
  if (!existsSync(local)) writeFileSync(local, make());
  const remote = `/sdcard/Download/${name}`;
  pushFile(serial, local, remote);
  shell(serial, ["touch", remote]);
  shell(serial, ["content", "call", "--uri", "content://media/external/file", "--method", "scan_file", "--arg", remote]);
  return name;
}

/** Random bytes, so no two runs stage the same preimage. */
function bytesOf(size) {
  return randomBytes(Math.round(size));
}

/** A seed for each run, so the noise images of two runs differ. */
const RUN_SEED = randomBytes(4).readUInt32BE(0);

/**
 * Pick `name` with the attachment button of the open room: the chooser,
 * then the file in the recent files of the system picker. With `notice`,
 * waits for the public-storage notice and gives the screen; without, fails
 * when the notice shows.
 */
async function pickFile(name, { notice }) {
  // A configuration change of the emulator while the picker shows can recreate the room screen, and the picked file
  // then does not reach it. The launcher picks once more and notes it.
  for (let attempt = 1; ; attempt += 1) {
    const mark = logMark();
    const screen = await pickFileOnce(name, { notice });
    // The worker starts again when the room shows again, and its PreimageSubmit prompt can cover the room. The room
    // gets the file only after the prompt closes.
    const picked = await waitFor(
      () => {
        dismissPrompts(parseUiNodes(uiDump(serial)));
        return logLines(mark).find((line) => line.message.startsWith("truapi.chat.attachment picked "));
      },
      { timeoutMs: 30_000, intervalMs: 500, message: "no picked file" },
    ).catch(() => undefined);
    if (picked) return screen ?? (notice ? waitScreen((nodes) => findText(nodes, "Files over 1 MB are public") && nodes, "the public-storage notice") : undefined);
    if (attempt >= 3) throw new Error(`the room did not get ${name} from the picker`);
    observations.push(`${new Date().toISOString().slice(11, 19)} ${name} did not reach the room from the picker; the launcher picked it again`);
    await openRoom(currentRoom);
  }
}

async function pickFileOnce(name, { notice }) {
  const room = currentRoom;
  // The caption is the name of the file, so the launcher finds a photo bubble too.
  await typeCaption(name);
  const screen = await waitScreen((nodes) => nodes.find((node) => node.description === "Attach") && nodes, "the attachment button");
  const attach = screen.find((node) => node.description === "Attach");
  inputTap(serial, attach.center.x, attach.center.y);
  // The room and the clipboard preview show the caption too, so the launcher takes only the rows of the system
  // chooser (`text1`) and of the document picker (`title`).
  const pickerFile = (node) => node.text === name && node.tag === "title";
  const chooserItem = (node) => node.tag === "text1" && /^(Photos & videos|Files)$/.test(node.text);
  // The chooser can take a tap while it still slides in and ignore it, so the launcher taps it again until the
  // document picker shows.
  let tapped = 0;
  const file = await waitFor(
    () => {
      const nodes = parseUiNodes(uiDump(serial));
      const found = nodes.find(pickerFile);
      if (found) return found;
      const item = nodes.find(chooserItem);
      if (item && Date.now() - tapped > 3_000) {
        inputTap(serial, item.center.x, item.center.y);
        tapped = Date.now();
      }
      return undefined;
    },
    { timeoutMs: COMMAND_TIMEOUT_MS, intervalMs: 500, message: `no ${name} in the picker` },
  );
  inputTap(serial, file.center.x, file.center.y);
  currentRoom = room;
  if (notice) {
    return waitScreen((nodes) => findText(nodes, "Files over 1 MB are public") && nodes, "the public-storage notice", 15_000)
      .catch(() => undefined);
  }
  return undefined;
}

/**
 * Put `text` into the input of the open room. The launcher does not send it.
 * The names of the files are plain ASCII, so `input text` types them: the
 * clipboard preview of a paste covers the attachment button.
 */
async function typeCaption(text) {
  expect(/^[A-Za-z0-9._-]+$/.test(text), `"${text}" is not a plain caption`);
  for (let attempt = 1; attempt <= 4; attempt += 1) {
    const screen = await waitScreen((nodes) => findTag(nodes, "chat_message_input") && nodes, "the message input");
    if (findTag(screen, "chat_message_input")?.text === text) return;
    const input = findTag(screen, "chat_message_input");
    inputTap(serial, input.center.x, input.center.y);
    inputKeyCombination(serial, KEY.ctrlLeft, KEY.a);
    inputKeyEvent(serial, KEY.del);
    // One key at a time: the app can lose or reorder the keys of a fast `input text`.
    for (const char of text) shell(serial, ["input", "text", char]);
    const typed = await waitScreen((nodes) => findTag(nodes, "chat_message_input")?.text === text && nodes, `"${text}" in the input`, 5_000)
      .catch(() => undefined);
    if (typed) {
      hideKeyboard();
      return;
    }
  }
  throw new Error(`could not type "${text}" into the input`);
}

/**
 * The banner of an attachment that the room did not send. The banner shows
 * for about 3 s, so a slow screen dump can miss it; the app then proves it
 * with the error that it logs as it shows the banner (`BaseViewModel`).
 */
async function attachmentBanner(mark, text, logged, seen) {
  if (seen === text) return `"${text}"`;
  const proof = await waitFor(
    () => {
      if (findText(parseUiNodes(uiDump(serial)), text)) return `"${text}"`;
      const line = logLines(mark).find((entry) => entry.tag === "BaseViewModel" && logged.test(entry.message));
      return line && `"${text}" (the banner went before the screen dump; the app logged ${line.message.split(": ").slice(-2).join(": ")})`;
    },
    { timeoutMs: COMMAND_TIMEOUT_MS, intervalMs: 300, message: `no "${text}" on screen or in the app log` },
  );
  return proof;
}

/** The lines of the stager since `mark`. */
function stageLines(mark) {
  return logLines(mark).filter((line) => /^truapi\.chat\.stage outbox:\S+ \d+ B in \d+ ms: /.test(line.message));
}

const PERMISSION_BANNER = "The file was not sent. Allow the app to store files on the Bulletin chain, then retry.";
const ATTACHMENT_BANNER = /^(The file was not sent|This file is too large to send)/;

/**
 * Wait for the next staging outcome, and give its logcat line with the
 * banner that the room shows for a failure (`banner`). The banner shows for
 * a few seconds only, so the launcher reads it while it waits. With
 * `failOnNotice`, the public-storage notice must not show meanwhile.
 */
async function waitStage(mark, what, { entryId, failOnNotice = false } = {}) {
  let banner;
  const look = () => {
    const screen = parseUiNodes(uiDump(serial));
    if (failOnNotice && findText(screen, "Files over 1 MB are public")) throw new Error("the notice showed again");
    banner ??= findText(screen, ATTACHMENT_BANNER)?.text;
    dismissPrompts(screen);
  };
  const line = await waitFor(
    () => {
      look();
      return stageLines(mark).find((entry) => !entryId || entry.message.includes(entryId));
    },
    { timeoutMs: 6 * 60_000, intervalMs: 500, message: `no ${what} in logcat` },
  );
  if (!/: Staged$/.test(line.message)) {
    await waitFor(() => (look(), banner), { timeoutMs: 8_000, intervalMs: 300, message: "no banner" }).catch(() => undefined);
  }
  return { ...line, banner };
}

/** The entries of the attachment outbox of the app. */
function outboxEntries() {
  const result = shell(serial, ["run-as", packageName, "ls", "files/product_attachment_outbox"]);
  return result.status === 0 ? result.stdout.split(/\s+/).filter(Boolean) : [];
}

/** Delete each failed attachment that the open room shows, from its menu. */
async function deleteOutboxBubbles() {
  for (let attempt = 0; attempt < 6 && outboxEntries().length > 0; attempt += 1) {
    const screen = parseUiNodes(uiDump(serial));
    dismissPrompts(screen);
    const bubble = screen.find((node) => /^s(5[7-9]|60)-.*\.(pdf|png)$/.test(node.text) && !node.tag);
    if (!bubble) break;
    inputLongPress(serial, bubble.center.x, bubble.center.y);
    const menu = await waitScreen((nodes) => findText(nodes, "Delete") && nodes, "the menu of the attachment").catch(() => undefined);
    if (!menu) continue;
    tapText(menu, "Delete");
    const confirm = await waitScreen((nodes) => nodes.filter((node) => node.text === "Delete").at(-1), "the delete confirmation");
    inputTap(serial, confirm.center.x, confirm.center.y);
    await delay(1_500);
  }
}

/** B7: the time that the bytes of an attachment take to cross the binding into the core. */
async function probeStaging(size) {
  const mark = logMark();
  hook([
    ["--es", "product_id", productHost],
    ["--ei", "stage_probe", String(size)],
  ]);
  const line = await waitForLog(mark, (entry) => entry.tag === E2E_TAG && entry.message.startsWith(`stage_probe bytes=${size} `), "the staging probe", 120_000);
  return line.message;
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
async function swipeRow(name, action) {
  const screen = await waitScreen((nodes) => findText(nodes, name) && nodes, `the row "${name}"`);
  const title = findText(screen, name);
  const y = title.center.y + 40;
  // A swipe that starts at an edge of the screen is the system Back gesture, so it starts inside.
  if (action === "mute") inputSwipe(serial, { x: 300, y }, { x: 1000, y }, 300);
  else inputSwipe(serial, { x: 850, y }, { x: 100, y }, 300);
  await delay(1_200);
}

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
    await swipeRow(ROOMS[room].name, "archive");
    await waitFor(() => Number(userStateRow(room)?.archived ?? 0) === 0, { timeoutMs: COMMAND_TIMEOUT_MS, message: "the unarchive is not stored" });
  }
  if (userStateRow(room)?.mutedUntil != null && Number(userStateRow(room).mutedUntil) > Date.now()) {
    await showChatList();
    await swipeRow(ROOMS[room].name, "mute");
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

// ------------------------------------------------------------- evidence

const DEVTOOLS_PORT = 9339;

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
    const roomsExist = () => roomRow("host") && roomRow("source");
    const firstTry = await waitFor(roomsExist, {
      timeoutMs: START_TIMEOUT_MS / 2,
      intervalMs: 2_000,
      message: "no rooms",
    }).catch(() => false);
    if (!firstTry) {
      // When the WebView renderer of a worker stops during the boot, the worker does not start again until the next demand.
      const crash = logLines(0).find((line) => /Renderer process \(\d+\) crash detected/.test(line.message));
      observations.push(
        `the product created no room in ${START_TIMEOUT_MS / 2000} s${crash ? ` after "${crash.message}"` : ""}; the launcher restarted the app`,
      );
      shell(serial, ["am", "force-stop", packageName]);
      amStart(serial, ["-n", launcherActivity]);
      await waitFor(roomsExist, {
        timeoutMs: START_TIMEOUT_MS,
        intervalMs: 2_000,
        message: `the product did not create its rooms in ${START_TIMEOUT_MS} ms after a restart; see app.log`,
      });
    }

    step("wait for the product to finish its start");
    await waitProductStarted();

    for (const scenario of ANDROID_CHAT_V2_SCENARIOS) {
      if (onlyScenarios.length > 0 && !onlyScenarios.includes(scenario.id)) continue;
      step(`${scenario.id} ${scenario.title}`);
      rows.push(await runScenario(scenario));
      console.log(`    ${rows.at(-1).result}: ${rows.at(-1).evidence}`);
    }
  } catch (error) {
    failure = error instanceof Error ? error.message : String(error);
    console.error(`\nthe run stopped: ${error instanceof Error ? error.stack : error}`);
    const remaining = ANDROID_CHAT_V2_SCENARIOS.filter(
      (scenario) =>
        !rows.some((existing) => existing.id === scenario.id) &&
        (onlyScenarios.length === 0 || onlyScenarios.includes(scenario.id)),
    );
    rows.push(...blockedRows(failure, remaining));
  } finally {
    rmSync(stagingDir, { recursive: true, force: true });
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

  const expected = ANDROID_CHAT_V2_SCENARIOS.filter(
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
