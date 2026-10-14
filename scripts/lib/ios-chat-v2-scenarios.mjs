// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

/**
 * The scenarios of the Chat Extension v2 test plan (04-TESTING section 4)
 * that name the iOS layer, in the order the iOS launcher reports them.
 *
 * A scenario with `drive` runs on the app of today: the launcher sends its
 * commands to the sample product and checks `expect`. A scenario with
 * `observe` also sends its command, but the app does not serve the call
 * yet, so its row is a `skip` that records how the call was refused. Every
 * other scenario is a `skip` that names the phase that adds it.
 */
export const IOS_CHAT_V2_SCENARIOS = Object.freeze([
  {
    id: "S2",
    title: "create_room V2 makes the host-store room",
    mode: "host",
    phase: "P3",
    drive: { commands: [], expect: { room: "Sample: host store" } },
    note: "the app keeps the name and icon; description, config and preview come with P3",
  },
  {
    id: "S1",
    title: "post_message V2 posts a product message in a host-store room",
    mode: "host",
    phase: "P3",
    drive: {
      commands: ["/post legacy"],
      expect: { text: "Legacy message from post_message" },
    },
  },
  {
    id: "S34",
    title: "footer Empty hides the input",
    mode: "host",
    phase: "P6",
    drive: {
      commands: ["/footer empty"],
      expect: { log: "[truapi:chat-bridge] setRoomFooter host" },
      after: ["/footer input"],
    },
  },
  {
    id: "S3",
    title: "set_room_metadata updates the list row and the header",
    mode: "host",
    phase: "P3",
  },
  {
    id: "S4",
    title: "every preview variant in the chat list",
    mode: "host",
    phase: "P3",
    observe: { command: "/preview pin", refusal: "setRoomPreview" },
  },
  {
    id: "S5",
    title: "delete_room removes the room",
    mode: "host",
    phase: "P3",
  },
  {
    id: "S6",
    title: "the chat list renders product rooms with the worker stopped",
    mode: "host",
    phase: "P3",
  },
  {
    id: "S7",
    title: "New renders every body kind",
    mode: "host",
    phase: "P4",
    observe: { command: "/post text", refusal: "updateFeed" },
  },
  { id: "S8", title: "Edited replaces in place", mode: "host", phase: "P4" },
  { id: "S9", title: "Deleted tombstones", mode: "host", phase: "P4" },
  { id: "S10", title: "reactions aggregate", mode: "host", phase: "P4" },
  { id: "S11", title: "StatusChanged marks", mode: "host", phase: "P4" },
  {
    id: "S12",
    title: "user events reach the product",
    mode: "host",
    phase: "P4",
  },
  {
    id: "S13",
    title: "an Actions press sends ActionTriggered",
    mode: "host",
    phase: "P4",
  },
  {
    id: "S14",
    title: "a command arrives as Command",
    mode: "host",
    phase: "P4",
  },
  {
    id: "S15",
    title: "product-source room opens on Before { None }",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S16",
    title: "scroll pages with the cursor",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S17",
    title: "a reply quote jumps across pages",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S18",
    title: "feed updates apply to the window",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S19",
    title: "send shows Preparing, then MessageCreated",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S20",
    title: "Rejected reverts the send",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S21",
    title: "no answer in 30 s marks Failed",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S22",
    title: "the window stays under 500 messages",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S23",
    title: "a killed worker keeps the cached pages",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S24",
    title: "RoomOpened and RoomClosed",
    mode: "source",
    phase: "P5",
  },
  { id: "S25", title: "ExtendBuiltIn menu", mode: "host", phase: "P6" },
  { id: "S26", title: "Custom menu", mode: "host", phase: "P6" },
  { id: "S27", title: "deeplink action", mode: "host", phase: "P6" },
  { id: "S28", title: "callback action", mode: "host", phase: "P6" },
  { id: "S29", title: "toolbar", mode: "host", phase: "P6" },
  { id: "S30", title: "participants", mode: "source", phase: "P6" },
  { id: "S31", title: "participant sheet", mode: "source", phase: "P6" },
  { id: "S32", title: "who reacted", mode: "source", phase: "P6" },
  {
    id: "S33",
    title: "footer Input honours placeholder and attachment toggles",
    mode: "host",
    phase: "P6",
  },
  { id: "S35", title: "footer Custom", mode: "host", phase: "P6" },
  {
    id: "S36",
    title: "inline attachment under 1 MiB",
    mode: "host",
    phase: "P6",
  },
  {
    id: "S37",
    title: "placeholder on an empty room",
    mode: "host",
    phase: "P6",
  },
  { id: "S38", title: "suggestion chips", mode: "host", phase: "P6" },
  { id: "S39", title: "the / catalogue", mode: "host", phase: "P6" },
  { id: "S40", title: "activity line", mode: "host", phase: "P6" },
  { id: "S41", title: "allowed_reactions", mode: "host", phase: "P6" },
  {
    id: "S44",
    title: "update_feed rate limit on screen",
    mode: "host",
    phase: "P4",
  },
  {
    id: "S46",
    title: "NotAvailable retry then default",
    mode: "source",
    phase: "P5",
  },
  {
    id: "S47",
    title: "configuration generation reload",
    mode: "host",
    phase: "P7",
  },
]);

/** The prefix the app's logger puts before each console line of a product. */
const JS_LINE = /JS: (\{"t":"chat-v2".*\})\s*$/;

/**
 * The event log lines of the sample product in the app's console output, as
 * objects. The launcher makes the Worker's `console.info` reach the logger.
 */
export function parseEventLog(text) {
  const entries = [];
  for (const line of text.split(/\r?\n/)) {
    const match = line.match(JS_LINE);
    if (!match) continue;
    try {
      entries.push(JSON.parse(match[1]));
    } catch {
      // A line the logger cut is not an entry.
    }
  }
  return entries;
}

/**
 * The reason the app gave for a call it does not serve, as the product saw
 * it: the `Rejected` reason that names `request`, or undefined.
 */
export function refusalReason(text, request) {
  const pattern = new RegExp(`this host does not serve chat ${request}`);
  const line = text.split(/\r?\n/).find((candidate) => pattern.test(candidate));
  return line
    ? `Rejected (this host does not serve chat ${request})`
    : undefined;
}

/**
 * Why the app did not start, from its console output, or undefined. The app
 * needs Firebase Remote Config for its chains and services: a build with
 * the placeholder `GoogleService-Info.plist` stops at "Configuration is
 * broken" before any product runs.
 */
export function appStartFailure(text) {
  if (/projects\/your-firebase-project-id\//.test(text)) {
    return 'the app has the placeholder GoogleService-Info.plist, so Firebase Remote Config fails and the app stops at "Configuration is broken"';
  }
  const failed = text
    .split(/\r?\n/)
    .find((line) => /FirebaseFacade\.handleFetchFailure/.test(line));
  return failed
    ? `Firebase Remote Config failed: ${failed.replace(/^.*handleFetchFailure\(\):\d+ - /, "").slice(0, 160)}`
    : undefined;
}

/**
 * The rows of the scenarios that the launcher could not run, because the
 * app did not start: the driven scenarios fail with `reason`, the others
 * keep their skip.
 */
export function blockedRows(reason, scenarios = IOS_CHAT_V2_SCENARIOS) {
  return scenarios.map((scenario) =>
    scenario.drive
      ? row(scenario, "fail", 0, `not run: ${reason}`)
      : skipRow(scenario),
  );
}

/** The `skip` row of a scenario that the app of today cannot serve. */
export function skipRow(scenario, observed) {
  const reason = `skip: needs ${scenario.phase}`;
  return row(scenario, "skip", 0, observed ? `${reason}; ${observed}` : reason);
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
