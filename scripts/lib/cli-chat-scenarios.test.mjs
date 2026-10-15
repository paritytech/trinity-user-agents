// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CLI_CHAT_SCENARIOS,
  renderScenarioReport,
  scenarioFailures,
} from "./cli-chat-scenarios.mjs";

const pass = (id) => ({
  id,
  title: id,
  mode: "host",
  result: "pass",
  durationMs: 5,
  evidence: `${id}.png`,
});

test("the CLI page covers the scenarios of layer L", () => {
  assert.deepEqual(CLI_CHAT_SCENARIOS.map((scenario) => scenario.id).sort(), [
    "S13",
    "S15",
    "S19",
    "S4",
    "S45",
    "S48",
    "S49",
    "S51",
    "S53",
    "S7",
    "S8",
  ]);
});

test("a run with every row passing has no failures", () => {
  assert.deepEqual(
    scenarioFailures(CLI_CHAT_SCENARIOS.map((scenario) => pass(scenario.id))),
    [],
  );
});

test("a failed row and a missing row both fail the run", () => {
  const rows = CLI_CHAT_SCENARIOS.slice(1).map((scenario) => pass(scenario.id));
  rows[0] = { ...rows[0], result: "fail", evidence: "no page" };
  assert.deepEqual(scenarioFailures(rows), [
    `${rows[0].id}: no page`,
    "S45: no row",
  ]);
});

test("the report has one row per scenario and escapes table cells", () => {
  const report = renderScenarioReport("CLI", [
    {
      id: "S7",
      title: "every | kind",
      mode: "host",
      result: "fail",
      durationMs: 12,
      evidence: "line one\nline two",
    },
  ]);
  assert.match(report, /^## CLI\n/);
  assert.match(report, /\*\*0 pass · 1 not passed\*\*/);
  assert.match(
    report,
    /\| S7 every \\\| kind \| host \| fail \| 12 ms \| line one line two \|/,
  );
});
