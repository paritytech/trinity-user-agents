// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The scenarios of the Chat Extension v2 test plan that the CLI chat page
 * covers (layer L), in the order the launcher drives them.
 */
export const CLI_CHAT_SCENARIOS = Object.freeze([
  {
    id: "S45",
    title: "a query right after start reaches the handler",
    mode: "source",
  },
  {
    id: "S15",
    title: "product-source room opens on Before { None }, newest at the bottom",
    mode: "source",
  },
  {
    id: "S19",
    title: "send shows Preparing, then the MessageCreated message",
    mode: "source",
  },
  { id: "S7", title: "every body kind renders", mode: "host" },
  {
    id: "S48",
    title: "Custom without a Renderer shows preview_text",
    mode: "host",
  },
  {
    id: "S13",
    title:
      "an Actions press sends ActionTriggered; the edit removes the buttons",
    mode: "host",
  },
  { id: "S8", title: "Edited replaces in place with edited", mode: "host" },
  { id: "S4", title: "every preview variant in the chat list", mode: "host" },
  {
    id: "S49",
    title:
      "capabilities lists exactly the features the page serves, and both bounds",
    mode: "host",
  },
  {
    id: "S51",
    title:
      "room features survive a worker restart; an unserved feature is stored and ignored",
    mode: "host",
  },
  {
    id: "S53",
    title:
      "register_bot answers New, then Exists and replaces the persona; a bot message draws its name and icon; an unknown persona is refused",
    mode: "host",
  },
  {
    id: "S54",
    title:
      "mute greys the badge, suppresses /notify into the room, and the product receives UserStateChanged",
    mode: "host",
  },
  {
    id: "S55",
    title:
      "archive moves the room to the archived list, where it stays after /post text; unarchive restores it",
    mode: "host",
  },
  {
    id: "S56",
    title:
      "a restarted worker receives UserStateChanged for every muted or archived room before live events",
    mode: "host",
  },
  {
    id: "S57",
    title:
      "a 3 MiB photo and a 6 MiB file are staged and arrive as Preimage media in both modes; the product reads them",
    mode: "host, source",
  },
  {
    id: "S58",
    title:
      "without preimage_attachments or above 8 MiB the attachment is refused and nothing is uploaded",
    mode: "host",
  },
]);

function cell(value) {
  return String(value).replace(/\s+/g, " ").replaceAll("|", "\\|").trim();
}

/**
 * The report of one run: one row per scenario, `pass`, `fail` with the first
 * assertion that failed, or `skip` with a reason (04-TESTING section 6).
 */
export function renderScenarioReport(title, rows) {
  const passed = rows.filter((row) => row.result === "pass").length;
  return (
    [
      `## ${title}`,
      "",
      `**${passed} pass · ${rows.length - passed} not passed**`,
      "",
      "| Scenario | Mode | Result | Duration | Evidence |",
      "| --- | --- | --- | --- | --- |",
      ...rows.map(
        (row) =>
          `| ${cell(`${row.id} ${row.title}`)} | ${cell(row.mode)} | ${row.result} | ${row.durationMs} ms | ${cell(row.evidence ?? "")} |`,
      ),
    ].join("\n") + "\n"
  );
}

/**
 * A line for each row that failed and each expected scenario with no row.
 * The launcher fails when this is not empty.
 */
export function scenarioFailures(rows, expected = CLI_CHAT_SCENARIOS) {
  const failures = rows
    .filter((row) => row.result === "fail")
    .map((row) => `${row.id}: ${row.evidence}`);
  for (const scenario of expected) {
    if (!rows.some((row) => row.id === scenario.id))
      failures.push(`${scenario.id}: no row`);
  }
  return failures;
}
