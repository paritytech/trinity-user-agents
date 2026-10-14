// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  IOS_CHAT_V2_SCENARIOS,
  appStartFailure,
  blockedRows,
  parseEventLog,
  refusalReason,
  skipRow,
} from "./ios-chat-v2-scenarios.mjs";

test("every iOS scenario of the test plan has one entry", () => {
  const ids = IOS_CHAT_V2_SCENARIOS.map((scenario) => scenario.id);
  assert.equal(new Set(ids).size, ids.length);
  for (const id of ["S1", "S2", "S7", "S15", "S34", "S41", "S47"]) {
    assert.ok(ids.includes(id), id);
  }
  for (const id of ["S42", "S43", "S45", "S48"]) {
    assert.ok(!ids.includes(id), `${id} has no iOS layer`);
  }
  for (const scenario of IOS_CHAT_V2_SCENARIOS) {
    assert.match(scenario.phase, /^P\d$/);
  }
});

test("the event log lines come out of the app console", () => {
  const console = [
    '17:22:07.337 💚 DEBUG JSEngineLogger.handle(body:):33 - JS: {"t":"chat-v2","boot":1,"seq":1,"via":"action_subscribe","room":"host","kind":"MessagePosted","data":{}}',
    "17:22:07.338 💚 DEBUG something else",
    '17:22:07.339 💚 DEBUG JSEngineLogger.handle(body:):33 - JS: {"t":"chat-v2","boot":1,',
  ].join("\n");
  assert.deepEqual(parseEventLog(console), [
    {
      t: "chat-v2",
      boot: 1,
      seq: 1,
      via: "action_subscribe",
      room: "host",
      kind: "MessagePosted",
      data: {},
    },
  ]);
});

test("a refusal names the request the app does not serve", () => {
  const console =
    "JS: chat-v2: reply in host failed CommandError: update_feed Rejected (this host does not serve chat updateFeed)";
  assert.equal(
    refusalReason(console, "updateFeed"),
    "Rejected (this host does not serve chat updateFeed)",
  );
  assert.equal(refusalReason(console, "setRoomPreview"), undefined);
});

test("the placeholder Firebase project stops the run with its reason", () => {
  const console =
    " - URL: https://firebaseinstallations.googleapis.com/v1/projects/your-firebase-project-id/installations/";
  assert.match(
    appStartFailure(console),
    /placeholder GoogleService-Info.plist/,
  );
  assert.equal(appStartFailure("all good"), undefined);
});

test("a blocked run fails the driven scenarios and keeps the skips", () => {
  const rows = blockedRows("no config");
  const driven = IOS_CHAT_V2_SCENARIOS.filter((scenario) => scenario.drive);
  assert.equal(rows.length, IOS_CHAT_V2_SCENARIOS.length);
  assert.equal(
    rows.filter((entry) => entry.result === "fail").length,
    driven.length,
  );
  assert.ok(rows.every((entry) => entry.result !== "pass"));
  assert.equal(
    rows.find((entry) => entry.id === "S1").evidence,
    "not run: no config",
  );
  assert.equal(
    rows.find((entry) => entry.id === "S9").evidence,
    "skip: needs P4",
  );
});

test("a skip row carries what the launcher observed", () => {
  const scenario = IOS_CHAT_V2_SCENARIOS.find((entry) => entry.id === "S7");
  assert.equal(
    skipRow(scenario, "refused").evidence,
    "skip: needs P4; refused",
  );
});
