// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import {
  IOS_CHAT_FEATURE_ROWS,
  IOS_CHAT_V2_SCENARIOS,
  appStartFailure,
  blockedRows,
  capabilityVerdict,
  capsLine,
  eventMatches,
  lastProductMessage,
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
    assert.match(scenario.phase, /^P\d+(\.\d+)?$/);
    assert.ok(
      Boolean(scenario.drive) !== Boolean(scenario.skip),
      `${scenario.id} is driven or names why it is not`,
    );
    for (const phase of scenario.drive?.phases ?? []) {
      assert.ok(phase.expect.length > 0, `${scenario.id} checks each phase`);
    }
  }
});

test("every scenario from S1 to S41 is driven", () => {
  const skipped = IOS_CHAT_V2_SCENARIOS.filter(
    (scenario) => scenario.skip && Number(scenario.id.slice(1)) <= 41,
  ).map((scenario) => scenario.id);
  assert.deepEqual(skipped, []);
});

test("S57 is a skip that names the bound of the live chain, and rows under it prove staging", () => {
  const byId = new Map(
    IOS_CHAT_V2_SCENARIOS.map((scenario) => [scenario.id, scenario]),
  );
  assert.match(byId.get("S57").skip, /2 MiB.*O32.*allowance/);
  for (const id of ["S57u-host", "S57u-source", "S58", "S59", "S60"]) {
    assert.ok(byId.get(id)?.drive, `${id} is driven`);
    assert.equal(byId.get(id).phase, "P10.3");
  }
  const modes = IOS_CHAT_V2_SCENARIOS.filter((scenario) =>
    scenario.id.startsWith("S57u"),
  ).map((scenario) => scenario.mode);
  assert.deepEqual(modes, ["host", "source"]);
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

test("a blocked run fails every driven scenario", () => {
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
});

test("a skip row carries its reason and what the launcher observed", () => {
  const scenario = IOS_CHAT_V2_SCENARIOS.find((entry) => entry.id === "S47");
  assert.equal(
    skipRow(scenario, "observed").evidence,
    "skip: needs P7; observed",
  );
});

test("the feature list of the launcher is the capability list of the app", () => {
  const swift = readFileSync(
    new URL(
      "../../hosts/ios/polkadot-app/Modules/Products/Chat/ChatHostCapabilities.swift",
      import.meta.url,
    ),
    "utf8",
  );
  const served = swift.match(/static let served: \[String\] = \[([^\]]*)\]/);
  assert.ok(served, "ChatHostCapabilities.served");
  const names = [...served[1].matchAll(/"([a-z_]+)"/g)].map(
    (match) => match[1],
  );
  assert.deepEqual(Object.keys(IOS_CHAT_FEATURE_ROWS), names);
});

test("each listed feature names driven rows that run before S49", () => {
  const ids = IOS_CHAT_V2_SCENARIOS.map((scenario) => scenario.id);
  const s49 = ids.indexOf("S49");
  for (const [name, rows] of Object.entries(IOS_CHAT_FEATURE_ROWS)) {
    for (const id of rows) {
      const index = ids.indexOf(id);
      assert.ok(index >= 0 && index < s49, `${name}: ${id} runs before S49`);
      assert.ok(IOS_CHAT_V2_SCENARIOS[index].drive, `${name}: ${id} is driven`);
    }
  }
  assert.equal(
    capsLine(["toolbar", "activity"]),
    "caps: toolbar, activity; inline 1048576 B; preimage 0 B",
  );
  assert.equal(capsLine([]), "caps: none; inline 1048576 B; preimage 0 B");
  assert.equal(
    capsLine(["room_links", "preimage_attachments"]),
    "caps: room_links, preimage_attachments; inline 1048576 B; preimage 8388608 B",
  );
});

test("S49 fails when a listed feature has a failed row", () => {
  const caps = { id: "S49", result: "pass", evidence: "stored" };
  const rows = [
    { id: "S29", result: "pass" },
    { id: "S32", result: "fail" },
  ];
  const features = { toolbar: ["S29"], reaction_details: ["S32"] };

  const verdict = capabilityVerdict(caps, rows, features);

  assert.equal(verdict.result, "fail");
  assert.match(verdict.evidence, /reaction_details \(S32 fail\)/);
});

test("S49 passes and names the rows that the run did not drive", () => {
  const caps = { id: "S49", result: "pass", evidence: "stored" };
  const features = { toolbar: ["S29"], activity: ["S40"] };

  const all = capabilityVerdict(
    caps,
    [
      { id: "S29", result: "pass" },
      { id: "S40", result: "pass" },
    ],
    features,
  );
  const partial = capabilityVerdict(
    caps,
    [
      { id: "S29", result: "pass" },
      { id: "S40", result: "skip" },
    ],
    features,
  );

  assert.equal(all.result, "pass");
  assert.match(all.evidence, /the rows of each listed feature passed/);
  assert.equal(partial.result, "pass");
  assert.match(
    partial.evidence,
    /not checked in this run: activity \(S40 skip\)/,
  );
  assert.equal(
    capabilityVerdict({ ...caps, result: "fail" }, [], features).result,
    "fail",
  );
});

test("the newest message of the product is never an edit row", () => {
  const markers = parseUiMarkers(
    "[e2e:ui] window host count=3 tail=host-p-0017:in-seen:text,host-p-0018:in-seen:text,host-p-0018#edit:in-seen:edited",
  );
  assert.equal(lastProductMessage(markers, "host"), "host-p-0018");
});

test("the P10.6 and P10.9 rows prove the features that the app lists", () => {
  const ids = new Set(IOS_CHAT_V2_SCENARIOS.map((scenario) => scenario.id));
  for (const name of [
    "bot_personas",
    "threads",
    "branches",
    "reasoning_rows",
    "read_aloud",
  ]) {
    assert.ok(IOS_CHAT_FEATURE_ROWS[name].length > 0, name);
    for (const id of IOS_CHAT_FEATURE_ROWS[name]) assert.ok(ids.has(id), id);
  }
});
