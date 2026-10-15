// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  IOS_CHAT_V2_SCENARIOS,
  appStartFailure,
  blockedRows,
  eventMatches,
  lastUserMessage,
  parseEventLog,
  parseUiMarkers,
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
    assert.ok(
      Boolean(scenario.drive) !== Boolean(scenario.skip),
      `${scenario.id} is driven or names why it is not`,
    );
    for (const phase of scenario.drive?.phases ?? []) {
      assert.ok(phase.expect.length > 0, `${scenario.id} checks each phase`);
    }
  }
});

test("every scenario from S1 to S41 is driven, except the ones the sample cannot drive", () => {
  const skipped = IOS_CHAT_V2_SCENARIOS.filter((scenario) => scenario.skip).map(
    (scenario) => scenario.id,
  );
  assert.deepEqual(skipped.sort(), ["S23", "S44", "S47", "S5", "S6"].sort());
});

test("the harness markers come out of the app console", () => {
  const console = [
    "19:46:47.448 💚 DEBUG ChatProductRoomPresenter.mark(_:_:):12 - [e2e:ui] window source count=51 tail=source-h-00120:in-seen:text,source-u-0001:out-sent:text",
    "19:46:47.449 💚 DEBUG something else",
    "19:46:47.450 💚 DEBUG ChatProductRoomPresenter.mark(_:_:):12 - [e2e:ui] toolbar host inline=tb-1,tb-2 overflow= participants=false",
  ].join("\n");
  const markers = parseUiMarkers(console);

  assert.deepEqual(
    markers.map((marker) => [marker.key, marker.room]),
    [
      ["window", "source"],
      ["toolbar", "host"],
    ],
  );
  assert.equal(lastUserMessage(markers, "source"), "source-u-0001");
  assert.equal(lastUserMessage(markers, "host"), undefined);
});

test("an expected event names its route, kind, room and data", () => {
  const entry = {
    via: "action_subscribe",
    kind: "MessagePosted",
    room: "host",
    data: { messageId: "", body: { text: "/help" } },
  };

  assert.ok(
    eventMatches(entry, { via: "action_subscribe", data: "/help" }, "host"),
  );
  assert.ok(!eventMatches(entry, { kind: "Command" }, "host"));
  assert.ok(!eventMatches(entry, { kind: "MessagePosted" }, "source"));
  assert.ok(
    !eventMatches(entry, { kind: "MessagePosted", hostId: true }, "host"),
  );
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
  assert.match(rows.find((entry) => entry.id === "S5").evidence, /^skip: /);
});

test("a skip row carries its reason and what the launcher observed", () => {
  const scenario = IOS_CHAT_V2_SCENARIOS.find((entry) => entry.id === "S47");
  assert.equal(
    skipRow(scenario, "observed").evidence,
    "skip: needs P7; observed",
  );
});
