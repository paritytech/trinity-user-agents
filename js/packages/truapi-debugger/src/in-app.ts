// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * In-app mount: render the inspector from a {@link DebugSession} that lives in
 * the SAME app as the host — no server, no dial-out, no relay. A host running in
 * the page (dotli) feeds each tapped frame via {@link InAppDebugger.handleFrame};
 * {@link InAppDebugger.mount} renders them with the same engine, the same
 * renderers, and the same stylesheet the standalone app uses.
 *
 * This is the "host and debugger in the same bits" transport: the frames never
 * leave the app, so each browser tab is its own tenant — nothing to host or
 * scope. Browser-only (uses `document`).
 *
 * Three consequences of sharing the app follow from that, and they are the
 * invariants this module holds:
 *
 *  - **The tap is in the product's frame path.** Feeding a frame can never throw
 *    into the caller, so {@link InAppDebugger.handleFrame} is contained.
 *  - **The feeding host is not this debugger's build.** dotli pins its own truapi
 *    dependencies, so a frame id may mean a different method here than it did
 *    there. Decode is therefore gated on an {@link InAppFrameIdentity} that
 *    affirmatively matches this build's wire schema — exactly the gate the
 *    standalone server applies to a dialing host. An unattested frame still
 *    groups (payload-blind grouping needs no contract) but never decodes.
 *  - **The document belongs to the host application.** Every shared rule is
 *    scoped to the mount root before injection (`scopeCss`), so the panel cannot
 *    restyle the host's own UI.
 *
 * The standalone app is a thin client over server-rendered fragments; this mount
 * has the session in-process, so it renders the same fragments directly and needs
 * no polling. Everything visible — the summary strip, the operation list, the
 * drill-down — comes from the shared renderers, the shared aggregate
 * ({@link computeTraceStats}), and the shared stylesheet, so the two mounts cannot
 * drift apart.
 *
 * @module
 */

import { TRUAPI_CODEC_VERSION, TRUAPI_WIRE_SCHEMA_HASH } from "@parity/truapi";
import {
  computeTraceStats,
  createDebugSession,
  decodeTraceFrames,
  formatStatBytes,
  formatStatMs,
  type TraceStats,
} from "./session.js";
import type { DebugSession, DebugSessionOptions } from "./session.js";
import { normalizeId, WIRE_ENVELOPE_VERSION } from "./ingest.js";
import {
  operationMethod,
  wireTraceToView,
  type TraceView,
} from "./trace-view.js";
import { renderOperationRow, renderTraceDetail } from "./trace-render.js";
import { detectRetryStorms } from "./retry-storm.js";
import { TRACE_DETAIL_CSS } from "./trace-styles.js";
import {
  INSPECTOR_LAYOUT_CSS,
  INSPECTOR_SHELL_CSS,
  scopeCss,
} from "./inspector-styles.js";

/** How an operation list is ordered. */
type SortMode = "arrival" | "slowest" | "frames";

/** The mount root's class; also the CSS scope every injected rule is confined to. */
const MOUNT_CLASS = "td-inapp";

/**
 * Retention caps for an embed, deliberately below the engine defaults the
 * standalone runs with. The standalone is its own process — if it retains a few
 * hundred MiB of traces, only the debugger pays. An embed retains inside the
 * observed application's own tab, where the same ceiling is the product's crash,
 * so the panel keeps a shorter, byte-bounded history.
 */
const EMBED_MAX_TRACES = 128;
/** @see EMBED_MAX_TRACES */
const EMBED_MAX_FRAMES_PER_TRACE = 256;
/** @see EMBED_MAX_TRACES */
const EMBED_MAX_BYTES_PER_TRACE = 256 * 1024;

/**
 * Cap on tracked channels, matching the standalone's registry: a host feeding
 * frames under many distinct channelIds must not grow the identity map without
 * bound.
 */
const MAX_CHANNELS = 256;

/**
 * The wire identity a feeding host stamps on a tapped frame — the same
 * `v`/`codec`/`schema` triple a dialing host puts on the standalone's envelope.
 *
 * A frame id is a `u8` discriminant that gets reassigned as the API evolves, so a
 * frame from a host built against a different wire table decodes to the WRONG
 * method and the wrong value off this debugger's table. The embed is the mount
 * most exposed to that: dotli pins its truapi dependencies independently of the
 * debugger's. Without an identity a frame is grouped but not decoded.
 */
export interface InAppFrameIdentity {
  /** Envelope version the feeder speaks; see {@link WIRE_ENVELOPE_VERSION}. */
  v?: number;
  /** The feeding host's wire codec version (`TRUAPI_CODEC_VERSION`). */
  codec?: number;
  /**
   * The feeding host's wire-contract fingerprint (`TRUAPI_WIRE_SCHEMA_HASH`): a
   * hash of every frame id and its method leg. This is the field decode is gated
   * on — unlike `codec` (the coarse handshake number, bumped ~never), it changes
   * whenever a frame id is reassigned.
   */
  schema?: string;
  /** Frames this tap dropped before this one; surfaced in the summary strip. */
  dropped?: number;
}

/** What one channel has declared about its wire contract, across all its frames. */
interface ChannelIdentity {
  /** `false` once a frame declared a `v`/`codec`/`schema` that differs. Sticky. */
  codecOk: boolean;
  /** Monotonic counter of the last frame seen, so eviction can pick the LRU. */
  lastSeen: number;
  /** `true` once a frame affirmatively declared a matching `schema`. */
  schemaOk: boolean;
  /** Frames the feeding tap reported dropping. */
  dropped: number;
}

/** A same-app debugger: feed it frames, mount its panel. */
export interface InAppDebugger {
  /** The underlying session — grouped traces, inline value decode. */
  readonly session: DebugSession;
  /**
   * Feed one tapped frame: the raw SCALE `ProtocolMessage` bytes, opaque. `dir`
   * is product-vantage (`out` = left the product), matching the standalone tap.
   *
   * `identity` is the feeder's wire contract. Pass it — a frame fed WITHOUT an
   * identity that matches this build's `TRUAPI_WIRE_SCHEMA_HASH` is grouped and
   * listed but never decoded, because its ids cannot be trusted to mean what this
   * debugger's table says they mean. Never throws: this runs inside the product's
   * own frame path.
   */
  handleFrame(
    channelId: string,
    dir: "in" | "out",
    frame: Uint8Array,
    identity?: InAppFrameIdentity,
  ): void;
  /**
   * Whether a decoded value may be surfaced for a channel's frames: it declared a
   * matching wire schema and never declared a mismatching identity. The panel
   * gates itself on this; an embedding host can read it to gate its own views.
   * Always `true` when the session has decode off (nothing decodes anyway).
   */
  decodeTrusted(channelId?: string): boolean;
  /**
   * Render a live, self-contained panel into `el` and keep it refreshed; returns
   * a disposer that tears the panel down. Decodes the open op's frames when the
   * session has `decodeValues` on AND the op's channel is
   * {@link InAppDebugger.decodeTrusted}.
   */
  mount(el: HTMLElement, options?: { refreshMs?: number }): () => void;
}

/** HTML-escape a string for interpolation into the markup this module builds. */
function esc(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/** One metric tile in the aggregate strip. */
function stat(n: string, k: string, sub = "", cls = ""): string {
  return (
    `<div class="ins-stat${cls === "" ? "" : ` ${cls}`}">` +
    `<span class="n">${esc(n)}` +
    (sub === "" ? "" : ` <span class="sub">${esc(sub)}</span>`) +
    `</span><span class="k">${esc(k)}</span></div>`
  );
}

/**
 * A tile that reads red when non-zero and RENDERS NOTHING at zero.
 *
 * These are exception counters: "0 truncated, 0 evicted, 0 dropped" is the
 * normal state, and a strip of zeros reads as unpopulated scaffolding rather
 * than as reassurance. A tile appearing is the signal; absence means zero.
 * Kept identical to the standalone's `warnTile` - the two strips must agree.
 */
function warnStat(n: number, k: string): string {
  if (n === 0) return "";
  return stat(String(n), k, "", "warn");
}

/** Neutral sibling of {@link warnStat}: a count worth showing that is not a fault. */
function infoStat(n: number, k: string): string {
  if (n === 0) return "";
  return stat(String(n), k, "", "");
}

/**
 * The aggregate summary strip: the "at a glance" row above the list.
 *
 * Every number comes from the shared {@link computeTraceStats}, and the tiles
 * mirror the standalone's strip one-for-one — same set, same labels, same
 * formatting. That is the whole point: a bespoke second roll-up here is how the
 * standalone came to report `malformed 1 / truncated 1` on a stream this mount
 * showed as clean.
 */
function renderSummary(stats: TraceStats): string {
  if (stats.ops === 0) return "waiting for frames…";

  const pills = stats.topMethods
    .map(
      ({ method, count }) =>
        `<span class="ins-method" data-method="${esc(method)}">` +
        `${esc(method)} <b>${String(count)}</b></span>`,
    )
    .join("");

  return (
    stat(String(stats.ops), "ops") +
    stat(
      String(stats.frames),
      "frames",
      `${String(stats.out)}▶ ${String(stats.in)}◀`,
    ) +
    stat(formatStatBytes(stats.bytes), "data") +
    (stats.subscriptions === 0
      ? ""
      : stat(
          String(stats.subscriptions),
          "subs",
          stats.liveSubscriptions > 0
            ? `${String(stats.liveSubscriptions)} live`
            : "",
        )) +
    stat(
      formatStatMs(stats.avgDurationMs),
      "avg op",
      `max ${formatStatMs(stats.maxDurationMs)}, observed`,
    ) +
    warnStat(stats.malformed, "malformed") +
    warnStat(stats.orphaned, "orphaned") +
    infoStat(stats.unpaired, "unpaired") +
    warnStat(stats.retryStorms, "retry storms") +
    warnStat(stats.truncated, "truncated") +
    warnStat(stats.evictedTraces, "evicted") +
    warnStat(stats.droppedByHost, "dropped") +
    (pills === "" ? "" : `<span class="ins-methods">${pills}</span>`)
  );
}

/** Stable identity for an op across refreshes: channel + id + generation. */
function opKey(view: TraceView): string {
  // Length-prefixed so a separator cannot be forged from inside a component. The
  // components are sender-controlled and only length-clamped (`normalizeId` filters
  // no characters), so a bare separator - a SPACE here, far likelier than NUL - let
  // channel "a" + request "b 0" collide with channel "a b" + request "0". Two ops
  // on different channels then shared one selection key: both rows highlighted and
  // the drill-down rendered whichever sorted first.
  const part = (v: string): string => `${String(v.length)}:${v}`;
  return `${part(view.channelId ?? "")} ${part(view.requestId)} ${part(String(view.generation ?? 0))}`;
}

/**
 * Create an in-app debugger. Decode is ON by default (dev-only tool) for frames
 * whose feeder attests a matching wire schema; pass `decodeValues: false` to keep
 * a bundled mount payload-blind regardless. Retention defaults to the
 * embed-appropriate caps ({@link EMBED_MAX_TRACES}); override any of them through
 * {@link DebugSessionOptions}.
 */
export function createInAppDebugger(
  options: DebugSessionOptions = {},
): InAppDebugger {
  const session = createDebugSession({
    ...options,
    maxTraces: options.maxTraces ?? EMBED_MAX_TRACES,
    maxFramesPerTrace: options.maxFramesPerTrace ?? EMBED_MAX_FRAMES_PER_TRACE,
    maxBytesPerTrace: options.maxBytesPerTrace ?? EMBED_MAX_BYTES_PER_TRACE,
  });

  const channels = new Map<string, ChannelIdentity>();
  /**
   * Channels evicted while carrying a mismatch verdict: a channel that declared
   * a foreign contract must never buy back trust simply by being forgotten.
   *
   * Add-only, never pruned, so it GROWS WITHOUT BOUND for the life of the tab -
   * one normalized channelId (<= 256 chars) per distinct channel that ever
   * mismatched. Measured 40 MiB at 200k such channels. Accepted rather than
   * capped because evicting from here is exactly the laundering it exists to
   * prevent; only mismatching channels feed it, so a well-behaved host
   * contributes nothing.
   */
  const distrusted = new Set<string>();
  /** Monotonic sequence for LRU ordering. */
  let seq = 0;
  // Sticky: some frame arrived unattested (or mismatched) this session. The
  // no-channel decode query keys on this rather than scanning the registry, whose
  // records can be evicted while the frames they described survive.
  let sawUnconfirmed = false;

  /**
   * Fold one frame's declared identity into its channel's record, and return
   * THIS frame's own verdict so the caller can stamp it on the envelope. The
   * channel record still drives the header chips and the "wire contract
   * differs" notice, but it must not be what decode keys on: as a latch, one
   * attested frame retroactively unlocked every unattested frame already
   * retained under that `channelId`.
   */
  const recordIdentity = (
    channelId: string,
    identity: InAppFrameIdentity | undefined,
  ): boolean => {
    const mismatch =
      (typeof identity?.v === "number" &&
        identity.v !== WIRE_ENVELOPE_VERSION) ||
      (typeof identity?.codec === "number" &&
        identity.codec !== TRUAPI_CODEC_VERSION) ||
      (typeof identity?.schema === "string" &&
        identity.schema !== TRUAPI_WIRE_SCHEMA_HASH);
    // Confirmed only by an affirmative match. An absent schema is NOT trusted:
    // "omit the identity and decode anyway" is the hole this closes.
    const confirmed = identity?.schema === TRUAPI_WIRE_SCHEMA_HASH;
    if (!confirmed || mismatch) sawUnconfirmed = true;
    // Same validation the standalone applies: a non-finite or fractional count
    // would otherwise render as "Infinity" or a rounded lie in the strip, and the
    // two mounts would disagree about the same feeder.
    const droppedRaw = identity?.dropped;
    const dropped =
      typeof droppedRaw === "number" &&
      Number.isSafeInteger(droppedRaw) &&
      droppedRaw > 0
        ? droppedRaw
        : 0;
    const key = normalizeId(channelId);
    const existing = channels.get(key);
    if (existing) {
      if (mismatch) existing.codecOk = false;
      if (confirmed) existing.schemaOk = true;
      existing.dropped += dropped;
      existing.lastSeen = seq++;
      // Re-insert so map order tracks recency: without this the map stays in
      // insertion order and the busiest, longest-lived channel is the FIRST
      // evicted under pressure.
      channels.delete(key);
      channels.set(key, existing);
      return confirmed && !mismatch;
    }
    if (channels.size >= MAX_CHANNELS) {
      // Evict the least recently seen, matching the standalone's registry.
      let oldestKey: string | undefined;
      let oldestSeen = Infinity;
      for (const [candidate, entry] of channels) {
        if (entry.lastSeen < oldestSeen) {
          oldestSeen = entry.lastSeen;
          oldestKey = candidate;
        }
      }
      if (oldestKey !== undefined) {
        const evicted = channels.get(oldestKey);
        channels.delete(oldestKey);
        // A mismatch verdict is sticky FOR THE SESSION, not for as long as the
        // entry survives. Forgetting it let a flood of distinct channelIds
        // launder a channel that had already declared a foreign wire contract:
        // it re-registered clean on its next frame and the panel decoded its
        // frames — wrong methods and wrong values, presented as truth.
        if (evicted !== undefined && !evicted.codecOk) {
          distrusted.add(oldestKey);
        }
      }
    }
    channels.set(key, {
      codecOk: !mismatch && !distrusted.has(key),
      schemaOk: confirmed,
      dropped,
      lastSeen: seq++,
    });
    return confirmed && !mismatch;
  };

  const decodeTrusted = (channelId?: string): boolean => {
    // Payload-blind mode never decodes, so the gate has nothing to guard.
    if (!session.decodeValues) return true;
    if (channelId !== undefined) {
      const c = channels.get(normalizeId(channelId));
      return c !== undefined && c.codecOk && c.schemaOk;
    }
    // No channel to key on: refuse once anything unattested has been seen.
    return !sawUnconfirmed;
  };

  /** Channels whose method names (and values) can't be trusted to this table. */
  const untrustedChannels = (): { any: boolean; mismatch: boolean } => {
    let any = false;
    let mismatch = false;
    for (const c of channels.values()) {
      if (!c.codecOk) {
        mismatch = true;
        any = true;
      } else if (!c.schemaOk) any = true;
    }
    return { any, mismatch };
  };

  return {
    session,
    decodeTrusted,
    handleFrame(channelId, dir, frame, identity) {
      // A debug tap must never disturb the frame path. In this mount that is not
      // a slogan: `handleFrame` is called from the host's own send/receive path in
      // the same call stack, so a throw here surfaces to the product as a
      // protocol failure. The standalone's socket callback carries the same guard
      // for the same reason; here the blast radius is larger.
      try {
        const frameConfirmed = recordIdentity(channelId, identity);
        // Stamp this frame with its own producer's verdict, so decode is gated
        // per frame rather than latched per channel. The verdict is
        // computed from the identity that arrived WITH this frame.
        session.handleEnvelope({
          channelId,
          dir,
          frame,
          identityConfirmed: frameConfirmed,
        });
      } catch {
        // Drop the frame; the observed session is worth more than one trace.
      }
    },
    mount(el, mountOptions = {}) {
      const style = document.createElement("style");
      // The shared rules are FLAT (`.ins-*`, `.td-*`) because that is right for
      // the standalone, which owns its page. Here they share a document with the
      // host application — an unscoped rule would restyle the host's own debug
      // panel, which `INSPECTOR_LAYOUT_CSS` is written to override — so every one
      // of them is rewritten to `.td-inapp <selector>` before injection. Only the
      // root rules below are written already-scoped.
      style.textContent = `
.${MOUNT_CLASS} { display: grid; grid-template-rows: auto auto minmax(0, 1fr) auto;
  height: 100%; min-height: 0; overflow: hidden; background: #0a0a0a; color: #e0e0e0;
  font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }
.${MOUNT_CLASS} * { box-sizing: border-box; }
${scopeCss(
  `${INSPECTOR_SHELL_CSS}\n${TRACE_DETAIL_CSS}\n${INSPECTOR_LAYOUT_CSS}`,
  `.${MOUNT_CLASS}`,
)}`;

      const root = document.createElement("div");
      root.className = MOUNT_CLASS;
      root.innerHTML = `
<div class="ins-top">
  <span class="ins-title">TrUAPI <span class="accent">Wire Inspector</span></span>
  <input class="ins-filter" type="search" placeholder="filter methods…" autocomplete="off" spellcheck="false">
  <select class="ins-sort" title="Sort operations">
    <option value="arrival">arrival</option>
    <option value="slowest">slowest</option>
    <option value="frames">frames</option>
  </select>
  <span class="ins-channels"></span>
</div>
<div class="ins-summary empty"></div>
<div class="ins-body">
  <div class="ins-list" tabindex="0"><div class="td-op-empty">waiting for frames…</div></div>
  <div class="ins-split"></div>
  <div class="ins-detail" tabindex="0"><div class="td-detail-empty">Select an operation to inspect its frames.</div></div>
</div>
<div class="ins-status"></div>`;
      el.append(style, root);

      const pick = <T extends Element>(selector: string): T => {
        const node = root.querySelector<T>(selector);
        if (node === null) throw new Error(`in-app mount: missing ${selector}`);
        return node;
      };
      const filterEl = pick<HTMLInputElement>(".ins-filter");
      const sortEl = pick<HTMLSelectElement>(".ins-sort");
      const channelsEl = pick<HTMLElement>(".ins-channels");
      const summaryEl = pick<HTMLElement>(".ins-summary");
      const listEl = pick<HTMLElement>(".ins-list");
      const detailEl = pick<HTMLElement>(".ins-detail");
      const statusEl = pick<HTMLElement>(".ins-status");

      let selected: string | null = null;
      let channel: string | null = null;
      let disposed = false;
      // Fingerprint of what the detail pane is currently showing. The refresh
      // ticks once a second, but re-rendering the open op means re-decoding and
      // re-hexing every one of its frames — tens of ms of blocked main thread, in
      // the product's own tab, for an op that has not changed. Skip unless the
      // fingerprint moves.
      let detailKey: string | null = null;

      const render = (): void => {
        if (disposed) return;
        const traces = session.traceEngine.traces();
        const storms = detectRetryStorms(traces);
        // One clock per render so every waiting op in this pass agrees, and the
        // 1s refresh makes a hung call visibly count up.
        const now = Date.now();
        const all = traces.map((trace) =>
          wireTraceToView(trace, session.methodNames, storms.get(trace) ?? []),
        );

        // Channel chips only earn their row once a second host has dialed in.
        const channelIds = [
          ...new Set(
            all
              .map((v) => v.channelId)
              .filter((c): c is string => c !== undefined),
          ),
        ];
        channelsEl.innerHTML =
          channelIds.length < 2
            ? ""
            : [null, ...channelIds]
                .map((c) => {
                  const active = c === channel ? " active" : "";
                  return (
                    `<button class="ins-chan${active}" data-channel="${esc(c ?? "")}">` +
                    `<span class="dot live"></span>${esc(c ?? "all")}</button>`
                  );
                })
                .join("");

        const scoped =
          channel === null ? all : all.filter((v) => v.channelId === channel);
        const untrusted = untrustedChannels();
        summaryEl.className = `ins-summary${scoped.length === 0 ? " empty" : ""}`;
        summaryEl.innerHTML = renderSummary(
          computeTraceStats(scoped, {
            evictedTraces: session.traceEngine.evictedTraces(),
            droppedByHost: [...channels.values()].reduce(
              (n, c) => n + c.dropped,
              0,
            ),
            codecMismatch: untrusted.mismatch,
          }),
        );

        const needle = filterEl.value.trim().toLowerCase();
        const filtered =
          needle === ""
            ? scoped
            : scoped.filter((v) =>
                // `operationMethod` is THE definition of an op's method and its
                // docstring requires every consumer to call it rather than
                // re-derive one, so the filter can never disagree with the label
                // the row renders. Re-deriving it off `frames[0]` hid rows whose
                // first observed frame was a closer.
                (operationMethod(v) ?? "").toLowerCase().includes(needle),
              );

        const sort = sortEl.value as SortMode;
        const ordered = [...filtered].sort((a, b) => {
          if (sort === "slowest") return b.durationMs - a.durationMs;
          if (sort === "frames") return b.frames.length - a.frames.length;
          return a.startedAt - b.startedAt;
        });

        // A row whose channel never attested a matching wire contract carries
        // method names resolved off THIS debugger's table, which may be wrong for
        // it. Say so above the rows, as the standalone does, rather than only in
        // the status bar.
        const listedUntrusted = ordered.some(
          (v) => !decodeTrusted(v.channelId),
        );
        const notice =
          !listedUntrusted || !session.decodeValues
            ? ""
            : `<div style="padding:4px 10px;color:#fca5a5;font-size:11px;border-bottom:1px solid rgba(255,255,255,.08)">⚠ ${
                untrusted.mismatch
                  ? "a feeding host's wire contract differs from this debugger's — method names below may be wrong and values are not decoded"
                  : "a feeding host declared no wire contract — method names below may be wrong and values are not decoded"
              }</div>`;
        listEl.innerHTML =
          ordered.length === 0
            ? `<div class="td-op-empty">${scoped.length === 0 ? "waiting for frames…" : "no operations match the filter"}</div>`
            : notice +
              ordered
                .map((view) => {
                  const row = renderOperationRow(view, { now });
                  return opKey(view) === selected
                    ? row.replace('class="td-op ', 'class="td-op selected ')
                    : row;
                })
                .join("");

        const open = ordered.find((v) => opKey(v) === selected);
        const trusted = open !== undefined && decodeTrusted(open.channelId);
        // Everything the detail render depends on, and nothing that ticks: frame
        // count and `lastAt` move whenever a frame lands, badges move when the op
        // changes shape, and the trust flag moves when an identity arrives.
        const nextDetailKey =
          open === undefined
            ? ""
            : [
                opKey(open),
                String(open.frames.length),
                String(open.lastAt),
                open.badges.join("|"),
                trusted ? "t" : "u",
              ].join(" ");
        if (nextDetailKey !== detailKey) {
          detailKey = nextDetailKey;
          detailEl.innerHTML =
            open === undefined
              ? `<div class="td-detail-empty">Select an operation to inspect its frames.</div>`
              : renderTraceDetail(open, {
                  offerDecode: session.decodeValues,
                  // Same wire-identity gate the standalone applies to a dialing
                  // host: an unattested channel groups but surfaces no value.
                  decoded: trusted
                    ? decodeTraceFrames(session, open)
                    : undefined,
                });
        }

        const evicted = session.traceEngine.evictedTraces();
        const identityWarning = untrusted.mismatch
          ? `<span class="mismatch" title="A feeding host declared a wire contract this debugger cannot decode against; value decode is refused for it.">⚠ codec mismatch</span>`
          : untrusted.any || (session.decodeValues && sawUnconfirmed)
            ? `<span class="mismatch" title="A feeding host did not declare a matching wire schema; value decode is refused for it.">⚠ wire identity unconfirmed</span>`
            : "";
        statusEl.innerHTML =
          `<span>${String(all.length)} ops</span>` +
          `<span class="live">in-app · decode ${session.decodeValues ? "on" : "off"}</span>` +
          (evicted > 0
            ? `<span class="mismatch">${String(evicted)} evicted</span>`
            : "") +
          identityWarning;
      };

      // Selecting a row is the only interaction that changes what the detail
      // pane shows, so re-render at once rather than waiting for the next tick.
      listEl.addEventListener("click", (event) => {
        const row = (event.target as HTMLElement).closest<HTMLElement>(
          ".td-op",
        );
        if (row === null) return;
        // Must match `opKey` exactly, INCLUDING its length prefixes: this rebuilds
        // the same key from the DOM, and a bare join would undo the collision fix
        // on the click path.
        const keyPart = (v: string): string => `${String(v.length)}:${v}`;
        const key = [
          keyPart(row.dataset["channelId"] ?? ""),
          keyPart(row.dataset["requestId"] ?? ""),
          keyPart(row.dataset["generation"] ?? "0"),
        ].join(" ");
        selected = selected === key ? null : key;
        render();
      });
      channelsEl.addEventListener("click", (event) => {
        const chip = (event.target as HTMLElement).closest<HTMLElement>(
          ".ins-chan",
        );
        if (chip === null) return;
        const value = chip.dataset["channel"] ?? "";
        channel = value === "" ? null : value;
        render();
      });
      summaryEl.addEventListener("click", (event) => {
        const pill = (event.target as HTMLElement).closest<HTMLElement>(
          ".ins-method",
        );
        if (pill === null) return;
        filterEl.value = pill.dataset["method"] ?? "";
        render();
      });
      filterEl.addEventListener("input", render);
      sortEl.addEventListener("change", render);

      render();
      const timer = setInterval(render, mountOptions.refreshMs ?? 1000);
      return () => {
        disposed = true;
        clearInterval(timer);
        style.remove();
        root.remove();
      };
    },
  };
}
