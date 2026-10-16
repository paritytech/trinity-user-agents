#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Drive one user of a run of `launch-android-verbosity.mjs` while it runs.
// This is the twin of `ios-verbosity.mjs`.
//
//   node scripts/android-verbosity.mjs <user> rooms
//   node scripts/android-verbosity.mjs <user> messages <room>
//   node scripts/android-verbosity.mjs <user> open <room>
//   node scripts/android-verbosity.mjs <user> send <room> <text>
//   node scripts/android-verbosity.mjs <user> press <room> <button-label> [text-of-the-message]
//   node scripts/android-verbosity.mjs <user> wait-room <pattern> [timeout-ms]
//   node scripts/android-verbosity.mjs <user> wait-message <room> <text> [timeout-ms]
//   node scripts/android-verbosity.mjs <user> screen
//   node scripts/android-verbosity.mjs <user> worker-log
//   node scripts/android-verbosity.mjs <user> background
//   node scripts/android-verbosity.mjs <user> foreground
//   node scripts/android-verbosity.mjs <user> screenshot <file>
//   node scripts/android-verbosity.mjs <user> whoami
//
// `<user>` is `a` or `b`. The state file is
// `playground/test-results/android-verbosity/state.json`, or
// `TRUAPI_ANDROID_E2E_VERBOSITY_OUT/state.json`.
//
// `background` presses Home, which sends the app to the background, and
// `foreground` brings it back. The core holds a foreground worker only while
// the app is in the foreground.

import { resolve } from "node:path";

import { EmulatorUser, readState } from "./lib/android-verbosity.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const outDir = resolve(
  repoRoot,
  process.env.TRUAPI_ANDROID_E2E_VERBOSITY_OUT ?? "playground/test-results/android-verbosity",
);
const [name, command, ...args] = process.argv.slice(2);
const state = readState(resolve(outDir, "state.json"));
const entry = state.users[name];
if (!entry) {
  throw new Error(`no user ${name}; the run has ${Object.keys(state.users)}`);
}
const user = new EmulatorUser(entry);

const commands = {
  rooms: () => user.rooms(),
  messages: (room) => user.messages(room),
  open: async (room) => {
    await user.open(room);
  },
  send: (room, ...words) => user.send(room, words.join(" ")),
  press: (room, label, ...near) => user.press(room, label, near.length > 0 ? near.join(" ") : undefined),
  "wait-room": (pattern, timeout) => user.waitForRoom(new RegExp(pattern), Number(timeout ?? 120_000)),
  "wait-message": (room, text, timeout) => user.waitForMessage(room, text, Number(timeout ?? 120_000)),
  screen: () => user.texts(),
  "worker-log": () => user.workerLog(),
  background: () => {
    user.background();
  },
  foreground: () => {
    user.foreground();
  },
  screenshot: (file) => user.screenshot(resolve(file)),
  whoami: () => ({ user: name, username: entry.username, device: entry.serial }),
};

const action = commands[command];
if (!action) {
  throw new Error(`unknown command ${command}; one of ${Object.keys(commands)}`);
}
const result = await action(...args);
if (result !== undefined) console.log(JSON.stringify(result, null, 2));
