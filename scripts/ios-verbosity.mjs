#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Drive one user of a run of `launch-ios-verbosity.mjs` while it runs.
//
//   node scripts/ios-verbosity.mjs <user> rooms
//   node scripts/ios-verbosity.mjs <user> messages <room>
//   node scripts/ios-verbosity.mjs <user> open <room>
//   node scripts/ios-verbosity.mjs <user> send <room> <text>
//   node scripts/ios-verbosity.mjs <user> press <room> <action-id> [message-id|last]
//   node scripts/ios-verbosity.mjs <user> action <room> <host-action> [argument] [target]
//   node scripts/ios-verbosity.mjs <user> wait-room <pattern> [timeout-ms]
//   node scripts/ios-verbosity.mjs <user> wait-message <room> <text> [timeout-ms]
//   node scripts/ios-verbosity.mjs <user> markers [key]
//   node scripts/ios-verbosity.mjs <user> worker-log
//   node scripts/ios-verbosity.mjs <user> background
//   node scripts/ios-verbosity.mjs <user> foreground
//   node scripts/ios-verbosity.mjs <user> screenshot <file>
//   node scripts/ios-verbosity.mjs <user> whoami
//
// `<user>` is `a` or `b`. The state file is
// `playground/test-results/ios-verbosity/state.json`, or
// `TRUAPI_IOS_E2E_VERBOSITY_OUT/state.json`.
//
// `background` opens the Settings app over the app, which sends the app to
// the background, and `foreground` brings it back. The core holds a
// foreground worker only while the app is in the foreground.

import { resolve } from "node:path";

import { DEFAULT_BUNDLE } from "./lib/ios-simulator.mjs";
import { SimulatorUser, readState } from "./lib/ios-verbosity.mjs";
import { run } from "./lib/process.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const outDir = resolve(
  repoRoot,
  process.env.TRUAPI_IOS_E2E_VERBOSITY_OUT ??
    "playground/test-results/ios-verbosity",
);
const [name, command, ...args] = process.argv.slice(2);
const state = readState(resolve(outDir, "state.json"));
const entry = state.users[name];
if (!entry) {
  throw new Error(`no user ${name}; the run has ${Object.keys(state.users)}`);
}
const user = new SimulatorUser(entry);
const bundle = process.env.TRUAPI_IOS_E2E_BUNDLE ?? DEFAULT_BUNDLE;

const commands = {
  rooms: () => user.rooms(),
  messages: (room) => user.messages(room),
  open: (room) => user.open(room),
  send: (room, ...words) => user.send(room, words.join(" ")),
  press: (room, actionId, message) => user.press(room, actionId, message),
  action: (room, host, argument, target) =>
    user.action(room, host, argument, target),
  "wait-room": (pattern, timeout) =>
    user.waitForRoom(new RegExp(pattern), Number(timeout ?? 120_000)),
  "wait-message": (room, text, timeout) =>
    user.waitForMessage(room, text, Number(timeout ?? 120_000)),
  markers: (key) => user.markers(key),
  "worker-log": () => user.workerLog(),
  background: () =>
    run("xcrun", ["simctl", "launch", entry.udid, "com.apple.Preferences"]),
  foreground: () => run("xcrun", ["simctl", "launch", entry.udid, bundle]),
  screenshot: (file) => user.screenshot(resolve(file)),
  whoami: () => ({ user: name, username: entry.username, device: entry.udid }),
};

const action = commands[command];
if (!action) {
  throw new Error(
    `unknown command ${command}; one of ${Object.keys(commands)}`,
  );
}
const result = await action(...args);
if (result !== undefined) console.log(JSON.stringify(result, null, 2));
