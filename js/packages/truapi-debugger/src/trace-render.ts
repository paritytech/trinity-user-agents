// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * The one drill-down renderer, mounted in both the standalone app and dotli's
 * panel.
 *
 * "One level deeper": given a selected op, render its frame sequence -
 * request→response, or subscribe→receive×N→stop - with method, direction, byte
 * length, latency, and orphaned/unpaired/malformed/retry-storm badges. It is a pure
 * `TraceView → HTML` function so the two mounts render identically; each mount
 * supplies the {@link TraceView} through its own adapter (see {@link
 * wireTraceToView} for the wire vantage).
 *
 * Payload-blind by default. Level-2 value decode is offered only when a mount
 * opts in (`offerDecode`) and passes decode results back in (`decoded`); the
 * renderer never touches bytes itself. Decode results come from the Core +
 * Decode thread's {@link FrameValueDetail}: a frame renders either its decoded
 * value or its byte length.
 *
 * The renderer emits HTML strings (both mounts assign `innerHTML`) using `td-*`
 * classes so one stylesheet covers both. Every interpolated string that came
 * off the wire (`requestId`, `method`) is escaped.
 *
 * @module
 */

import type { FrameValueDetail } from "./decode.js";
import {
  isLiveSubscription,
  isSubscription,
  operationMethod,
} from "./trace-view.js";
import type {
  TraceBadge,
  TraceFrameBadge,
  TraceFrameView,
  TraceView,
} from "./trace-view.js";
import type { TraceDropCounts } from "./wire-debugger.js";

/** Options controlling a single drill-down render. */
export interface RenderTraceDetailOptions {
  /**
   * Offer the per-frame level-2 decode affordance for decodable frames. Off by
   * default: the view stays payload-blind and shows no decode control.
   */
  offerDecode?: boolean;
  /**
   * Decoded values for this op, keyed by frame `seq`. A dev-only mount decodes
   * every frame up front (calling the Core session's `frameDetail`) and passes
   * the results here. A frame absent from the map falls back to its byte length.
   */
  decoded?: ReadonlyMap<number, FrameValueDetail>;
}

/** HTML-escape a wire-sourced string before it touches `innerHTML`. */
function esc(value: string): string {
  return value.replace(/[&<>"']/g, (c) => {
    switch (c) {
      case "&":
        return "&amp;";
      case "<":
        return "&lt;";
      case ">":
        return "&gt;";
      case '"':
        return "&quot;";
      default:
        return "&#39;";
    }
  });
}

/**
 * Compact duration: `42` → `42ms`, `1234` → `1.23s`, `205_000` → `3m 25s`,
 * `10_800_000` → `3h 00m`.
 *
 * Seconds cannot be the largest unit: this also formats how long an unanswered
 * call has been waiting, and a session left open renders "10800.00s" - a number
 * nobody reads as three hours.
 */
function formatMs(ms: number): string {
  if (ms < 1000) return `${String(Math.round(ms))}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)}s`;
  const pad = (n: number): string => String(n).padStart(2, "0");
  const totalSeconds = Math.floor(ms / 1000);
  if (ms < 3_600_000) {
    return `${String(Math.floor(totalSeconds / 60))}m ${pad(totalSeconds % 60)}s`;
  }
  const totalMinutes = Math.floor(totalSeconds / 60);
  return `${String(Math.floor(totalMinutes / 60))}h ${pad(totalMinutes % 60)}m`;
}

const DIRECTION_GLYPH: Record<TraceFrameView["direction"], string> = {
  out: "▶",
  in: "◀",
};

/**
 * Render the drill-down detail for one op. Returns an HTML fragment for a
 * mount's detail pane (`.td-detail` in dotli, the detail column in the app).
 */
export function renderTraceDetail(
  view: TraceView,
  options: RenderTraceDetailOptions = {},
): string {
  const offerDecode = options.offerDecode ?? false;
  const decoded = options.decoded;

  const header = renderHeader(view);
  const rows = view.frames
    .map((frame) => renderFrameRow(frame, offerDecode, decoded?.get(frame.seq)))
    .join("");

  return (
    `<div class="td-trace" data-request-id="${esc(view.requestId)}">` +
    header +
    `<div class="td-frames" role="list">${rows}</div>` +
    `</div>`
  );
}

function renderHeader(view: TraceView): string {
  const badges = view.badges
    .map((b) => renderOpBadge(b, view.dropped))
    .join("");
  const frameCount = view.frames.length;
  return (
    `<div class="td-trace-head">` +
    `<code class="td-trace-id">${esc(view.requestId)}</code>` +
    `<span class="td-trace-meta">${String(frameCount)} frame${frameCount === 1 ? "" : "s"} · ${formatMs(view.durationMs)}</span>` +
    (badges === "" ? "" : `<span class="td-trace-badges">${badges}</span>`) +
    `</div>`
  );
}

const OP_BADGE_LABEL: Record<TraceBadge, string> = {
  orphaned: "orphaned",
  unpaired: "unpaired",
  malformed: "malformed",
  "retry-storm": "retry storm",
  truncated: "truncated",
};

function renderOpBadge(badge: TraceBadge, dropped?: TraceDropCounts): string {
  // `truncated` carries a count when the vantage supplies one, so "1 frame lost"
  // and "77 lost" don't render identically.
  const label =
    badge === "truncated" && dropped !== undefined
      ? `truncated ${String(droppedTotal(dropped))}`
      : OP_BADGE_LABEL[badge];
  return `<span class="td-badge td-badge-${badge}" title="${esc(badgeTitle(badge, dropped))}">${esc(label)}</span>`;
}

/** Frames missing plus payloads shed: everything the caps took from this op. */
function droppedTotal(dropped: TraceDropCounts): number {
  return dropped.framesByCount + dropped.framesByBytes + dropped.payloadsShed;
}

/** Spell out which cap took what, so the two axes are distinguishable. */
function truncationTitle(dropped: TraceDropCounts): string {
  const parts: string[] = [];
  if (dropped.framesByCount > 0) {
    parts.push(`${String(dropped.framesByCount)} frames dropped (frame cap)`);
  }
  if (dropped.framesByBytes > 0) {
    parts.push(`${String(dropped.framesByBytes)} frames dropped (byte cap)`);
  }
  if (dropped.payloadsShed > 0) {
    parts.push(
      `${String(dropped.payloadsShed)} payloads shed (single frame over the byte cap; frame kept)`,
    );
  }
  return parts.length === 0
    ? "Older frames were dropped to stay under the frame/byte cap"
    : parts.join(" · ");
}

function badgeTitle(badge: TraceBadge, dropped?: TraceDropCounts): string {
  switch (badge) {
    case "orphaned":
      return "An opening frame has no matching close - it went out and nothing came back";
    case "unpaired":
      return "A closing frame with no opener observed - an op that began before the debugger attached, a close the engine outlived, or a second close. Not a host fault on its own";
    case "malformed":
      return "A frame failed to decode on the wire";
    case "retry-storm":
      return "This op is one of a burst of like ops in a short window";
    case "truncated":
      return dropped === undefined
        ? "Older frames were dropped to stay under the frame/byte cap"
        : truncationTitle(dropped);
  }
}

const FRAME_BADGE_LABEL: Record<TraceFrameBadge, string> = {
  malformed: "malformed",
  orphaned: "orphaned",
  unpaired: "unpaired",
};

function renderFrameRow(
  frame: TraceFrameView,
  offerDecode: boolean,
  detail: FrameValueDetail | undefined,
): string {
  const glyph = DIRECTION_GLYPH[frame.direction];
  const method =
    frame.method === undefined
      ? `<span class="td-frame-method anon">id ${String(frame.frameId ?? "?")}</span>`
      : `<span class="td-frame-method">${esc(frame.method)}</span>`;
  const role = `<span class="td-frame-role td-role-${frame.role}">${esc(frame.role)}</span>`;
  const size =
    frame.byteLength === undefined
      ? ""
      : `<span class="td-frame-bytes">${String(frame.byteLength)}B</span>`;
  const latency = renderLatency(frame);
  const badges = frame.badges
    .map(
      (b) =>
        `<span class="td-frame-badge td-badge-${b}">${esc(FRAME_BADGE_LABEL[b])}</span>`,
    )
    .join("");

  // The frame's meta (direction, role, method, size, latency, badges) is one
  // grouped cell so a mount can pin the level-2 payload into a fixed second
  // column beside it - every frame's decoded box then opens in the same aligned
  // space rather than trailing variable-width meta.
  const meta =
    `<div class="td-frame-meta">` +
    `<span class="td-frame-dir td-dir-${frame.direction}">${glyph}</span>` +
    role +
    method +
    size +
    latency +
    (badges === "" ? "" : `<span class="td-frame-badges">${badges}</span>`) +
    `</div>`;

  const payload =
    offerDecode && frame.decodable
      ? `<div class="td-frame-payload">${renderDecodeBlock(frame, detail)}</div>`
      : "";

  return (
    `<div class="td-frame" data-seq="${String(frame.seq)}" role="listitem">` +
    meta +
    payload +
    `</div>`
  );
}

function renderLatency(frame: TraceFrameView): string {
  // A closing frame that answers an opener shows its round-trip; everything
  // else shows its offset from the op's first frame.
  if (frame.roundTripMs !== undefined) {
    return `<span class="td-frame-latency" title="round trip to the frame it answers — debugger-observed, includes transport/queueing delay">⟳ ${formatMs(frame.roundTripMs)}</span>`;
  }
  if (frame.latencyFromStartMs === 0) {
    return `<span class="td-frame-latency td-latency-start">+0</span>`;
  }
  return `<span class="td-frame-latency" title="offset from the op's first frame — debugger-observed, includes transport/queueing delay">+${formatMs(frame.latencyFromStartMs)}</span>`;
}

/**
 * The level-2 payload slot for one frame. A dev-only tool decodes every frame,
 * so this shows the decoded value; a frame whose value could not be resolved
 * (bytes not retained, or a decode miss) shows its byte length instead.
 */
function renderDecodeBlock(
  frame: TraceFrameView,
  detail: FrameValueDetail | undefined,
): string {
  if (detail !== undefined) {
    return `<div class="td-frame-decoded">${renderFrameValueDetail(detail)}</div>`;
  }
  const size =
    frame.byteLength === undefined ? "" : `${String(frame.byteLength)}B · `;
  return `<div class="td-bytes-only">${size}payload not shown</div>`;
}

/**
 * Render a Core-thread {@link FrameValueDetail}. Shared by both mounts so the
 * outcome is identical everywhere: a frame shows its decoded value, or its byte
 * length when no value is available.
 */
export function renderFrameValueDetail(detail: FrameValueDetail): string {
  switch (detail.kind) {
    case "bytes":
      // Show the raw hex when we have it (dev-only: nothing is hidden); only a
      // frame with no retained bytes reads "payload not shown".
      return detail.hex !== undefined
        ? `<pre class="td-detail-pre">${String(detail.byteLength)}B · ${esc(detail.hex)}</pre>`
        : `<div class="td-bytes-only">${String(detail.byteLength)}B · payload not shown</div>`;
    case "decoded":
      return `<pre class="td-detail-pre">${esc(stringifyValue(detail.value))}</pre>`;
  }
}

/** Pretty-print a decoded value for a `<pre>`, tolerating cyclic/bigint inputs. */
function stringifyValue(value: unknown): string {
  try {
    // `JSON.stringify` returns the JS `undefined` (not a string) for `undefined`
    // and functions - a real decoded shape now that a paramless subscription's
    // `Start` leg decodes to bare `undefined`, with no wrapper to report.
    return JSON.stringify(
      value,
      (_key, v: unknown) => (typeof v === "bigint" ? `${v.toString()}n` : v),
      2,
    ) ?? String(value);
  } catch {
    return String(value);
  }
}

/**
 * Whether the op went out and nothing came back: an *opening* frame carrying the
 * `orphaned` badge. This is the shape a timed-out or hung call takes on the wire
 * - there is no "timeout" frame to observe, only a request with no reply - so it
 * is the signal the op list has to surface as elapsed time.
 *
 * The role check is now belt-and-braces rather than load-bearing: `orphaned` is
 * opener-only by construction, since a closer with no opener earns `unpaired`
 * instead. It used to be essential - the badge fired on both, and the
 * shapes it caught are often perfectly live: a `receive` that arrived after the
 * `stop`, a subscription the debugger attached to mid-session and only ever saw
 * receives of, an opener whose frame id was off this debugger's table. Reading the
 * op badge as "unanswered" reported a subscription delivering a frame a second as
 * "waiting 300s", and turned a completed 120ms round trip into "waiting 120s".
 * Kept so this predicate stays correct on its own terms if the badge derivation
 * ever widens again.
 */
function isUnanswered(view: TraceView): boolean {
  return view.frames.some(
    (f) =>
      (f.role === "request" || f.role === "start") &&
      f.badges.includes("orphaned"),
  );
}

/**
 * Render one operation-list row: the primary view's unit, one per op. Shows the
 * method, a request/subscription glyph, op-level badges, frame count, and
 * duration. A subscription with no `stop` frame is marked live.
 *
 * Pure and stateless: the mount toggles `.selected` and manages the keyed diff.
 * `data-request-id` (+ `data-channel-id` when known) identify the row for
 * selection and channel filtering. Payload-blind: only shape and timing here.
 */
export function renderOperationRow(
  view: TraceView,
  options: { now?: number } = {},
): string {
  const method = operationMethod(view);
  const sub = isSubscription(view);
  // Liveness comes from the canonical predicate: a subscription the host ended
  // with an `interrupt` is not live either, and counting it as live inflates the
  // live-subscription total for the rest of the session.
  const live = isLiveSubscription(view);
  const kindGlyph = sub ? "⟳" : "▶";
  const kindClass = sub ? "td-op-sub" : "td-op-req";

  // `.td-op-method` is truncated on the left (`direction: rtl`), which reorders
  // any label that is not a pure LTR identifier: `account.getAccount:` renders as
  // `:account.getAccount` and `22.getAccount` as `getAccount.22`, because `.`,
  // `:` and digits are direction-neutral. An explicit LTR isolate around the
  // method keeps it a single left-to-right run while the ellipsis stays on the
  // left, where the whole point of the rtl trick is to put it.
  const methodHtml =
    method === undefined
      ? `<span class="td-op-method anon">(unknown)</span>`
      : `<span class="td-op-method" title="${esc(method)}"><bdi dir="ltr">${esc(method)}</bdi></span>`;
  const badges = view.badges
    .map((b) => renderOpBadge(b, view.dropped))
    .join("");
  const count = view.frames.length;
  // An unanswered request has one frame, so `lastAt - startedAt` is 0 and the op
  // reads "0ms" - the opposite of the truth for the case a developer most needs
  // to see, a call that went out and is still hanging. Report the age of the
  // request instead, so a stuck op counts up rather than looking instant.
  const waiting = isUnanswered(view) && options.now !== undefined;
  const meta = waiting
    ? `${String(count)} frame${count === 1 ? "" : "s"} · waiting ${formatMs(
        Math.max(0, (options.now ?? 0) - view.startedAt),
      )}`
    : `${String(count)} frame${count === 1 ? "" : "s"} · ` +
      (live
        ? `live · ${formatMs(view.durationMs)}`
        : formatMs(view.durationMs));

  const channelAttr =
    view.channelId === undefined
      ? ""
      : ` data-channel-id="${esc(view.channelId)}"`;
  // Generation disambiguates ops that recycle a `(channelId, requestId)`; the
  // client keys rows and the drill-down on it so reused ids stay distinct.
  const genAttr = ` data-generation="${String(view.generation ?? 0)}"`;

  return (
    `<div class="td-op ${kindClass}${live ? " td-op-live" : ""}${waiting ? " td-op-waiting" : ""}" ` +
    `data-request-id="${esc(view.requestId)}"${channelAttr}${genAttr} role="listitem" tabindex="-1">` +
    `<span class="td-op-kind" aria-hidden="true">${kindGlyph}</span>` +
    methodHtml +
    (badges === "" ? "" : `<span class="td-op-badges">${badges}</span>`) +
    `<span class="td-op-meta">${meta}</span>` +
    `</div>`
  );
}
