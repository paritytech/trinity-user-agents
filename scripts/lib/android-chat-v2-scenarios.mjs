// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The scenarios of the Chat Extension v2 test plan (04-TESTING section 4)
 * that name the Android layer, in the order the Android launcher runs them.
 *
 * A scenario with `drive: true` runs on the app of today: the launcher has a
 * driver for it, sends the commands of the sample product and checks the
 * result. Every other scenario is a `skip` row with its `skip` reason, or
 * with the phase that adds it.
 *
 * The launcher runs the product-source scenarios before the host-store ones,
 * so that S14 can show the `Seen` reports of the product-source room. The
 * rows of the Chat Extension v2 additions (P10) come after the P8 rows, and
 * S49 comes after every row that proves a feature that the app can list.
 * The launcher runs S4 and S6 last, because they leave the room screen. S5
 * and S44 are `skip` rows: the sample product has no command that drives
 * them.
 */
export const ANDROID_CHAT_V2_SCENARIOS = Object.freeze([
  {
    id: "S2",
    title: "create_room V2 persists metadata, config and preview",
    mode: "both",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S15",
    title: "product-source room opens on Before { None }, newest at the bottom",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S17",
    title: "a reply quote jumps to the referenced message",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S16",
    title: "scroll pages with the cursor until next: None, no page twice",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S18",
    title: "feed updates apply to the window",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S11",
    title: "StatusChanged through Sent, Delivered, Read, Failed; Failed offers retry",
    mode: "source",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S19",
    title: "send shows Preparing, then MessageCreated",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S20",
    title: "Rejected reverts the send and shows the reason",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S21",
    title: "no answer in 30 s marks Failed; retry re-sends",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S46",
    title: "NotAvailable is retried once after 1 s",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S30",
    title: "participants: count in the header, pages, authors by id",
    mode: "source",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S31",
    title: "participant sheet: entries of the kind, product actions",
    mode: "source",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S32",
    title: "who reacted from reactions_query; NotAvailable hides it",
    mode: "source",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S22",
    title: "the window stays under 500 messages",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S24",
    title: "RoomOpened and RoomClosed; the worker stops after close",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S23",
    title: "a killed worker keeps the cached pages; retry re-acquires",
    mode: "source",
    phase: "P8.5",
    drive: true,
  },
  {
    id: "S1",
    title: "post_message V2 posts a product message in a host-store room",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S3",
    title: "set_room_metadata updates the header without reopening",
    mode: "host",
    phase: "P8.3",
    drive: true,
  },
  {
    id: "S7",
    title: "New renders every body kind",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S8",
    title: "Edited replaces in place with the edited mark",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S9",
    title: "Deleted tombstones; the list preview recomputes",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S10",
    title: "reactions of a product participant and of the user aggregate",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S12",
    title: "user send, reply, edit, delete, react reach the product",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S13",
    title: "an Actions press sends ActionTriggered; the edit removes the buttons",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S14",
    title: "a declared command arrives as Command; report_seen sends Seen",
    mode: "host",
    phase: "P8.4",
    drive: true,
  },
  {
    id: "S25",
    title: "ExtendBuiltIn menu: the defaults, minus Edit when hidden, plus product actions",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S26",
    title: "Custom menu shows only product actions; a query time-out shows the defaults",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S28",
    title: "a callback action sends MessageActionTriggered",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S27",
    title: "a deeplink action asks for confirmation and navigates",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S29",
    title: "toolbar: two inline, the rest in the overflow; a press sends ToolbarActionTriggered",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S33",
    title: "footer Input honours placeholder and attachment toggles",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S36",
    title: "inline attachment under 1 MiB; a larger one is refused",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S34",
    title: "footer Empty hides the input",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S35",
    title: "footer Custom draws the product's tree; a press reaches the product",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S37",
    title: "placeholder on an empty room",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S38",
    title: "suggestion chips: sends_text sends, the other sends SuggestionSelected",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S39",
    title: "the / catalogue completes commands and sends Command",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S40",
    title: "activity line shows, clears on None and on RoomClosed",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S41",
    title: "allowed_reactions limits the picker",
    mode: "host",
    phase: "P8.6",
    drive: true,
  },
  {
    id: "S47",
    title: "a configuration generation change while the room is open reloads the policy",
    mode: "host",
    phase: "P8.8",
    drive: true,
  },
  {
    id: "S61",
    title: "in-room search in a host-store room finds by text, case and diacritics ignored, and jumps",
    mode: "host",
    phase: "P10.5",
    drive: true,
  },
  {
    id: "S62",
    title: "product-source search pages search_query hits; /search na shows not available; no action without search",
    mode: "source",
    phase: "P10.5",
    skip: "skip: needs P10.5.1 sample (/search on|na|off and its search_query handler), then the search capability",
  },
  {
    id: "S54",
    title: "mute for 1 hour greys the badge, suppresses /notify, and the product receives UserStateChanged",
    mode: "host",
    phase: "P10.10",
    drive: true,
  },
  {
    id: "S55",
    title: "archive moves the room to the archived list, it stays after /post text, unarchive restores it",
    mode: "host",
    phase: "P10.10",
    drive: true,
  },
  {
    id: "S56",
    title: "a restarted worker receives UserStateChanged for every muted or archived room first",
    mode: "both",
    phase: "P10.10",
    drive: true,
  },
  {
    id: "S63",
    title: "the list search finds product rooms by name and host-store text and starts no worker",
    mode: "both",
    phase: "P10.5",
    drive: true,
  },
  {
    id: "S49",
    title: "capabilities lists exactly the features whose rows pass, and both byte bounds",
    mode: "host",
    phase: "P10.8",
    drive: true,
  },
  {
    id: "S4",
    title: "every preview variant in the chat list",
    mode: "host",
    phase: "P8.3",
    drive: true,
  },
  {
    id: "S6",
    title: "the chat list renders product rooms with the worker stopped",
    mode: "host",
    phase: "P8.3",
    drive: true,
  },
  {
    id: "S5",
    title: "delete_room removes the room",
    mode: "host",
    phase: "P8.4",
    skip: "skip: the sample product has no command that calls delete_room on this base (playground/chat-v2/README.md, Commands); RealDeleteRoomUseCaseTest covers the delete path",
  },
  {
    id: "S44",
    title: "update_feed rate limit on screen",
    mode: "host",
    phase: "P8.8",
    skip: "skip: no command of the sample product sends more than 20 updates in a second; the core tests the bound, and S8 shows the coalesced edits of /stream at 20 a second",
  },
]);

/** The logcat tag of the console lines of a product WebView. */
export const CONSOLE_TAG = "ProductWebChromeClientKt";

const LOGCAT_LINE =
  /^\d{2}-\d{2} (\d{2}:\d{2}:\d{2}\.\d{3})\s+(\d+)\s+(\d+)\s+([VDIWEF])\s+(\S.*?)\s*:\s?(.*)$/;

/** One `threadtime` logcat line as an object, or undefined. */
export function parseLogcatLine(line) {
  const match = LOGCAT_LINE.exec(line ?? "");
  if (!match) return undefined;
  const [, time, pid, tid, level, tag, message] = match;
  return { time, pid, tid, level, tag, message };
}

const EVENT_START = /^Script [^:]+: (\{"t":"chat-v2".*)$/;
const CONSOLE_SOURCE = / \[[^\]\s]*:\d+\]$/;
/** The most logcat lines that one event log line can take. */
const MAX_CHUNKS = 256;

/**
 * The event log lines of the sample product in the logcat lines of the app,
 * as objects. The app logs each console line of a product WebView as
 * `Script <product>: <text> [<source>:<line>]`, and Timber cuts a message
 * longer than 4,000 characters into lines of the same thread, so a long
 * event log line comes back from more than one logcat line.
 */
export function parseEventLog(lines) {
  const entries = [];
  const pending = new Map();
  const finish = (tid) => {
    const open = pending.get(tid);
    if (!open) return;
    pending.delete(tid);
    const entry = parseEventText(open.text);
    if (entry) entries.push(entry);
  };
  for (const line of lines) {
    const parsed = parseLogcatLine(line);
    if (!parsed || parsed.tag !== CONSOLE_TAG) continue;
    const start = EVENT_START.exec(parsed.message);
    if (start) {
      finish(parsed.tid);
      const entry = parseEventText(start[1]);
      if (entry) entries.push(entry);
      else pending.set(parsed.tid, { text: start[1], chunks: 1 });
      continue;
    }
    const open = pending.get(parsed.tid);
    if (!open) continue;
    if (parsed.message.startsWith("Script ")) {
      finish(parsed.tid);
      continue;
    }
    open.text += parsed.message;
    open.chunks += 1;
    const entry = parseEventText(open.text);
    if (entry) {
      pending.delete(parsed.tid);
      entries.push(entry);
    } else if (open.chunks >= MAX_CHUNKS) {
      pending.delete(parsed.tid);
    }
  }
  return entries;
}

function parseEventText(text) {
  try {
    return JSON.parse(text.replace(CONSOLE_SOURCE, ""));
  } catch {
    return undefined;
  }
}

/** The text of the console lines of a product WebView, without the source. */
export function consoleMessages(lines) {
  const messages = [];
  for (const line of lines) {
    const parsed = parseLogcatLine(line);
    if (parsed?.tag === CONSOLE_TAG) {
      messages.push(parsed.message.replace(CONSOLE_SOURCE, ""));
    }
  }
  return messages;
}

const NODE = /<node\b([^>]*?)\/?>/g;
const ATTRIBUTE = /([\w:-]+)="([^"]*)"/g;
const BOUNDS = /^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$/;

/**
 * The nodes of a `uiautomator dump`, each with its text, content
 * description, resource id (a Compose test tag), class and bounds.
 */
export function parseUiNodes(xml) {
  const nodes = [];
  for (const match of (xml ?? "").matchAll(NODE)) {
    const attributes = {};
    for (const [, name, value] of match[1].matchAll(ATTRIBUTE)) {
      attributes[name] = decodeXml(value);
    }
    const bounds = BOUNDS.exec(attributes.bounds ?? "");
    if (!bounds) continue;
    const [left, top, right, bottom] = bounds.slice(1).map(Number);
    const resourceId = attributes["resource-id"] ?? "";
    nodes.push({
      text: attributes.text ?? "",
      description: attributes["content-desc"] ?? "",
      tag: resourceId.replace(/^.*:id\//, ""),
      className: attributes.class ?? "",
      clickable: attributes.clickable === "true",
      bounds: { left, top, right, bottom },
      center: { x: (left + right) / 2, y: (top + bottom) / 2 },
    });
  }
  return nodes;
}

function decodeXml(value) {
  return value.replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos);/gi, (entity, body) => {
    if (body[0] === "#") {
      const code = body[1] === "x" || body[1] === "X"
        ? Number.parseInt(body.slice(2), 16)
        : Number(body.slice(1));
      return String.fromCodePoint(code);
    }
    return { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" }[body.toLowerCase()] ?? entity;
  });
}

/** The scenarios in the order of their number, for the report. */
export function byScenarioNumber(rows) {
  const number = (row) => Number(row.id.replace(/^S/, ""));
  return [...rows].sort((a, b) => number(a) - number(b));
}

/** The `skip` row of a scenario that the launcher does not drive. */
export function skipRow(scenario, observed) {
  const reason = scenario.skip ?? `skip: needs ${scenario.phase}`;
  return row(scenario, "skip", 0, observed ? `${reason}; ${observed}` : reason);
}

/**
 * The rows of the scenarios that the launcher could not run: the driven
 * scenarios fail with `reason`, the others keep their skip.
 */
export function blockedRows(reason, scenarios = ANDROID_CHAT_V2_SCENARIOS) {
  return scenarios.map((scenario) =>
    scenario.drive ? row(scenario, "fail", 0, `not run: ${reason}`) : skipRow(scenario),
  );
}

export function row(scenario, result, durationMs, evidence) {
  return {
    id: scenario.id,
    title: scenario.title,
    mode: scenario.mode,
    result,
    durationMs: Math.round(durationMs),
    evidence,
  };
}
