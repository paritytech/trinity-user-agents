// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  ANDROID_CHAT_V2_SCENARIOS,
  blockedRows,
  byScenarioNumber,
  consoleMessages,
  parseEventLog,
  parseLogcatLine,
  parseUiNodes,
  skipRow,
} from "./android-chat-v2-scenarios.mjs";

const console = (tid, message) =>
  `10-14 20:14:38.393  9539  ${tid} D ProductWebChromeClientKt: ${message}`;

test("every Android scenario of the test plan has one entry", () => {
  const ids = ANDROID_CHAT_V2_SCENARIOS.map((scenario) => scenario.id);
  assert.equal(new Set(ids).size, ids.length);
  for (let number = 1; number <= 41; number += 1) {
    assert.ok(ids.includes(`S${number}`), `S${number}`);
  }
  for (const id of ["S42", "S43", "S45", "S48"]) {
    assert.ok(!ids.includes(id), `${id} has no Android layer`);
  }
  for (const scenario of ANDROID_CHAT_V2_SCENARIOS) {
    assert.match(scenario.phase, /^P(8|10)\.\d+$/);
  }
});

test("S1 to S24 and S46 are driven, the P8.6 scenarios are skips", () => {
  const driven = ANDROID_CHAT_V2_SCENARIOS.filter((scenario) => scenario.drive).map((scenario) => scenario.id);
  for (let number = 1; number <= 24; number += 1) {
    if (number === 5) continue;
    assert.ok(driven.includes(`S${number}`), `S${number}`);
  }
  assert.ok(driven.includes("S46"));
  for (let number = 25; number <= 41; number += 1) {
    const scenario = ANDROID_CHAT_V2_SCENARIOS.find((entry) => entry.id === `S${number}`);
    assert.equal(skipRow(scenario).evidence, "skip: needs P8.6");
  }
});

test("the P10.10 and P10.5 rows are driven, and S62 waits for the sample", () => {
  const byId = (id) => ANDROID_CHAT_V2_SCENARIOS.find((scenario) => scenario.id === id);
  for (const id of ["S54", "S55", "S56", "S61", "S63"]) {
    assert.equal(byId(id).drive, true, id);
  }
  assert.match(skipRow(byId("S62")).evidence, /^skip: needs P10\.5\.1 sample/);
});

test("the P10.6 and P10.9 rows are driven, S68 with the product-source rooms", () => {
  const byId = (id) => ANDROID_CHAT_V2_SCENARIOS.find((scenario) => scenario.id === id);
  for (const id of ["S53", "S64", "S65", "S66", "S67", "S68"]) {
    assert.equal(byId(id).drive, true, id);
  }
  assert.equal(byId("S68").mode, "source");
  const order = ANDROID_CHAT_V2_SCENARIOS.map((scenario) => scenario.id);
  assert.ok(order.indexOf("S68") < order.indexOf("S1"), "S68 runs with the product-source rows");
});

test("a scenario with its own skip reason keeps it", () => {
  const scenario = ANDROID_CHAT_V2_SCENARIOS.find((entry) => entry.id === "S5");
  assert.match(skipRow(scenario).evidence, /^skip: the sample product has no command that calls delete_room/);
});

test("the event log lines come out of the console lines of the worker", () => {
  const lines = [
    console(
      "9539",
      'Script truapi-chat-v2.dot: {"t":"chat-v2","boot":2,"seq":1,"via":"action_subscribe","room":"host","kind":"RoomOpened","data":null} [http://127.0.0.1:3100/worker/index.js:6306]',
    ),
    "10-14 20:14:38.394  9539  9539 D OtherTag: {\"t\":\"chat-v2\",\"boot\":9}",
    console("9539", "Script truapi-chat-v2.dot: chat-v2: action_subscribe ended [http://127.0.0.1:3100/worker/index.js:6642]"),
  ];
  assert.deepEqual(parseEventLog(lines), [
    { t: "chat-v2", boot: 2, seq: 1, via: "action_subscribe", room: "host", kind: "RoomOpened", data: null },
  ]);
  assert.deepEqual(consoleMessages(lines), [
    'Script truapi-chat-v2.dot: {"t":"chat-v2","boot":2,"seq":1,"via":"action_subscribe","room":"host","kind":"RoomOpened","data":null}',
    "Script truapi-chat-v2.dot: chat-v2: action_subscribe ended",
  ]);
});

test("a long event log line that Timber cut into chunks comes back whole", () => {
  const text = JSON.stringify({ t: "chat-v2", boot: 1, seq: 7, via: "answer", re: 6, room: "source", kind: "Page", data: { messages: ["x".repeat(30)] } });
  const full = `Script truapi-chat-v2.dot: ${text} [http://127.0.0.1:3100/worker/index.js:6306]`;
  const lines = [
    console("100", full.slice(0, 50)),
    console("200", 'Script truapi-chat-v2.dot: {"t":"chat-v2","boot":1,"seq":8,"via":"render","room":null,"kind":"ChatFooter","data":null} [x:1]'),
    console("100", full.slice(50, 90)),
    console("100", full.slice(90)),
  ];
  const entries = parseEventLog(lines);
  assert.deepEqual(entries.map((entry) => entry.seq), [8, 7]);
  assert.deepEqual(entries[1].data, { messages: ["x".repeat(30)] });
});

test("a cut line that never completes is dropped at the next event line", () => {
  const lines = [
    console("100", 'Script truapi-chat-v2.dot: {"t":"chat-v2","boot":1,"seq":1,'),
    console("100", 'Script truapi-chat-v2.dot: {"t":"chat-v2","boot":1,"seq":2,"via":"answer","room":"source","kind":"Page","data":null} [x:1]'),
  ];
  assert.deepEqual(parseEventLog(lines).map((entry) => entry.seq), [2]);
});

test("a logcat line splits into its fields", () => {
  assert.deepEqual(parseLogcatLine("10-14 20:14:06.472  9539  9753 D truapi.core: truapi.ws_bridge.connection_open: 127.0.0.1:43182"), {
    time: "20:14:06.472",
    pid: "9539",
    tid: "9753",
    level: "D",
    tag: "truapi.core",
    message: "truapi.ws_bridge.connection_open: 127.0.0.1:43182",
  });
  assert.equal(parseLogcatLine("--------- beginning of main"), undefined);
});

test("the nodes of a uiautomator dump carry text, tag and bounds", () => {
  const xml = `<?xml version='1.0' encoding='UTF-8' standalone='yes' ?><hierarchy rotation="0">
<node index="0" text="/react last &#128077;" resource-id="io.parity.polkadotapp.debug:id/chat_message_input" class="android.widget.EditText" content-desc="" clickable="true" bounds="[66,2120][925,2252]" />
<node index="1" text="" resource-id="" class="android.view.View" content-desc="Inline sample image" clickable="false" bounds="[44,1989][824,2340]"><node index="0" text="Tom &amp; Jerry" resource-id="chat_send_button" class="android.widget.Button" content-desc="" clickable="true" bounds="[926,2120][1058,2252]" /></node>
</hierarchy>`;
  const nodes = parseUiNodes(xml);
  assert.equal(nodes.length, 3);
  assert.equal(nodes[0].text, "/react last 👍");
  assert.equal(nodes[0].tag, "chat_message_input");
  assert.deepEqual(nodes[0].center, { x: 495.5, y: 2186 });
  assert.equal(nodes[1].description, "Inline sample image");
  assert.equal(nodes[2].text, "Tom & Jerry");
  assert.equal(nodes[2].tag, "chat_send_button");
});

test("the report rows sort by scenario number", () => {
  const rows = [{ id: "S15" }, { id: "S2" }, { id: "S1" }, { id: "S46" }];
  assert.deepEqual(byScenarioNumber(rows).map((row) => row.id), ["S1", "S2", "S15", "S46"]);
});

test("a run that could not start fails the driven rows and keeps the skips", () => {
  const rows = blockedRows("the app did not start");
  const s7 = rows.find((row) => row.id === "S7");
  const s25 = rows.find((row) => row.id === "S25");
  assert.equal(s7.result, "fail");
  assert.equal(s7.evidence, "not run: the app did not start");
  assert.equal(s25.result, "skip");
});
