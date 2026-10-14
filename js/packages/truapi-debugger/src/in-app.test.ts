import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  encodeWireMessage,
  MESSAGE_TYPE_REQUEST,
  VersionedHostAccountGetRequest,
  TRUAPI_CODEC_VERSION,
  TRUAPI_WIRE_SCHEMA_HASH,
} from "@parity/truapi";
import * as REAL_W from "@parity/truapi/wire-table";
import { Window } from "happy-dom";

import { createInAppDebugger, type InAppFrameIdentity } from "./in-app.js";
import { WIRE_ENVELOPE_VERSION } from "./ingest.js";
import {
  INSPECTOR_LAYOUT_CSS,
  INSPECTOR_SHELL_CSS,
} from "./inspector-styles.js";
import { computeTraceStats, createDebugSession } from "./session.js";
import { renderOperationRow } from "./trace-render.js";
import { frameIdOf } from "./wire-debugger.js";
import { wireTraceToView } from "./trace-view.js";

/**
 * Test-only convenience view over the generated wire table: gives each entry
 * per-leg "flat id" properties by encoding `(frameId, messageType)` as one
 * number (`frameId * 4 + messageType`), so fixtures can address a method's
 * specific leg by name. `frameBytes()` below decodes a leg id back into a
 * real `(trait, method)` pair and a `messageType` byte.
 */
function legacyIds(table: Record<string, unknown>): Record<string, Record<string, number>> {
  const legs = {
    request: ["request", "response"],
    subscription: ["start", "receive", "interrupt", "stop"],
  } as const;
  const out: Record<string, Record<string, number>> = {};
  for (const [name, entry] of Object.entries(table)) {
    if (
      entry === null ||
      typeof entry !== "object" ||
      !("trait" in entry) ||
      !("method" in entry) ||
      !("kind" in entry)
    ) {
      continue;
    }
    const { trait, method, kind } = entry as {
      trait: number;
      method: number;
      kind: "request" | "subscription";
    };
    const base = frameIdOf(trait, method) * 4;
    const rec: Record<string, number> = {};
    legs[kind].forEach((leg, i) => {
      rec[leg] = base + i;
    });
    out[name] = rec;
  }
  return out;
}

const W = legacyIds(REAL_W as unknown as Record<string, unknown>);

/** The leg's own payload bytes (`value`), for leg id `legId`. */
function frameBytes(
  legId: number,
  value: number[] = [0],
  requestId = "p:1",
): Uint8Array {
  const frameId = Math.floor(legId / 4);
  const messageType = legId % 4;
  const r = encodeWireMessage({
    requestId,
    payload: {
      traitId: Math.floor(frameId / 256),
      methodId: frameId % 256,
      messageType,
      value: new Uint8Array(value),
    },
  });
  if (r.isErr()) throw r.error;
  return r.value;
}

/** A real, decodable account-get request wire message (non-sensitive). */
function accountGetRequestBytes(requestId = "p:1"): Uint8Array {
  const value = VersionedHostAccountGetRequest.enc({
    tag: "V1",
    value: {
      productAccountId: {
        dotNsIdentifier: "alice.dot",
        derivationIndex: { tag: "Index", value: 0 },
      },
    },
  });
  const r = encodeWireMessage({
    requestId,
    payload: {
      traitId: REAL_W.ACCOUNT_GET_ACCOUNT.trait,
      methodId: REAL_W.ACCOUNT_GET_ACCOUNT.method,
      messageType: MESSAGE_TYPE_REQUEST,
      value,
    },
  });
  if (r.isErr()) throw r.error;
  return r.value;
}

/**
 * The wire identity a feeding host stamps when it was built against THIS
 * debugger's wire table — the only state in which the panel decodes a payload.
 */
const ATTESTED: InAppFrameIdentity = {
  v: WIRE_ENVELOPE_VERSION,
  codec: TRUAPI_CODEC_VERSION,
  schema: TRUAPI_WIRE_SCHEMA_HASH,
};

/** Selector lists of every rule in a stylesheet (no nested at-rules in ours). */
function ruleSelectors(css: string): string[] {
  return [...css.matchAll(/([^{}]+)\{[^{}]*\}/g)].map((m) =>
    (m[1] ?? "").trim(),
  );
}

describe("createInAppDebugger", () => {
  // The mount is a real interactive panel now (querySelector, dataset, event
  // listeners), so it needs a real DOM rather than a stand-in.
  /* eslint-disable @typescript-eslint/no-explicit-any -- install a DOM global */
  const g = globalThis as any;
  const original = g.document;
  let win: Window;
  beforeAll(() => {
    win = new Window();
    g.document = win.document;
  });
  afterAll(() => {
    g.document = original;
  });
  /* eslint-enable @typescript-eslint/no-explicit-any */

  /** A detached container to mount into. */
  const container = (): HTMLElement =>
    win.document.createElement("div") as unknown as HTMLElement;

  /**
   * A container attached to the document, so `getComputedStyle` resolves the
   * mount's injected stylesheet against it.
   */
  const attached = (): HTMLElement => {
    const el = container();
    win.document.body.append(el as never);
    return el;
  };

  /** Click the first operation row of a mounted panel. */
  const openFirstOp = (el: HTMLElement): void => {
    const row = el.querySelector<HTMLElement>(".td-op");
    expect(row).not.toBeNull();
    row?.click();
  };

  test("feeds frames in-process and decodes by default", () => {
    const dbg = createInAppDebugger(); // decode ON by default (dev-only tool)

    // Two frames of one op, fed exactly as dotli's tap would (raw SCALE bytes).
    // The request leg carries a real, decodable account-get payload.
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    expect(dbg.session.traceEngine.traces()).toHaveLength(1);
    expect(dbg.session.decodeValues).toBe(true); // decodes by default

    // The drill-down surfaces the decoded value.
    const detail = dbg.session.frameDetail("p:1", 0, "shop.dot");
    expect(detail?.kind).toBe("decoded");

    const el = container();
    const dispose = dbg.mount(el);
    // Rendered by the shared renderer — the method resolved via the wire table.
    expect(el.querySelector(".ins-list")?.innerHTML).toContain(
      "account.getAccount",
    );
    dispose();
    expect(el.children).toHaveLength(0);
  });

  test("a formerly-sensitive op shows its payload like any other (no redaction)", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame(
      "shop.dot",
      "out",
      frameBytes(W.SIGNING_SIGN_RAW.request, [1, 2]),
      ATTESTED,
    );
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.SIGNING_SIGN_RAW.response),
      ATTESTED,
    );

    const el = attached();
    const dispose = dbg.mount(el);
    openFirstOp(el);
    const html = el.querySelector(".ins-detail")?.innerHTML ?? "";

    // No denylist: the panel shows this op's payload — decoded, or the raw hex
    // when the codec can't type it — exactly as it would any other method.
    expect(html).toContain("signing.signRaw");
    expect(html).toContain(`<pre class="td-detail-pre">`);
    expect(html).not.toContain("payload not shown");
    expect(html.toLowerCase()).not.toContain("redact");

    dispose();
    el.remove();
  });

  test("decodeValues:false keeps the mount payload-blind (bytes only)", () => {
    const dbg = createInAppDebugger({ decodeValues: false });
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    expect(dbg.session.decodeValues).toBe(false);
    expect(dbg.session.frameDetail("p:1", 0, "shop.dot")?.kind).toBe("bytes");
  });

  test("the mount renders the full inspector chrome, not a bare list", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    const el = container();
    const dispose = dbg.mount(el);

    // The pieces that made the standalone look like a Network tab and were
    // absent here: a top bar with filter + sort, the aggregate strip, the
    // list/detail split, and a status bar.
    for (const selector of [
      ".ins-top",
      ".ins-filter",
      ".ins-sort",
      ".ins-summary",
      ".ins-body",
      ".ins-list",
      ".ins-split",
      ".ins-detail",
      ".ins-status",
    ]) {
      expect(el.querySelector(selector)).not.toBeNull();
    }
    // The strip reports real aggregates rather than a placeholder.
    expect(el.querySelector(".ins-summary")?.innerHTML).toContain("ops");
    expect(el.querySelector(".ins-summary")?.className).not.toContain("empty");

    dispose();
  });

  test("selecting an operation opens its frames in the detail pane", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    const el = container();
    const dispose = dbg.mount(el);

    const detail = el.querySelector(".ins-detail");
    expect(detail?.innerHTML).toContain("Select an operation");

    openFirstOp(el);

    // The drill-down replaces the placeholder, and the row reads as selected.
    expect(detail?.innerHTML).not.toContain("Select an operation");
    expect(detail?.innerHTML).toContain("account.getAccount");
    expect(el.querySelector(".td-op.selected")).not.toBeNull();

    dispose();
  });

  test("the filter narrows the operation list", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    const el = container();
    const dispose = dbg.mount(el);
    expect(el.querySelectorAll(".td-op")).toHaveLength(1);

    const filter = el.querySelector<HTMLInputElement>(".ins-filter");
    expect(filter).not.toBeNull();
    if (filter !== null) {
      filter.value = "signing.signRaw";
      filter.dispatchEvent(new win.Event("input") as unknown as Event);
    }
    expect(el.querySelectorAll(".td-op")).toHaveLength(0);
    expect(el.querySelector(".ins-list")?.innerHTML).toContain("no operations");

    dispose();
  });

  // --- wire identity ------------------------------------------------------

  test("an unattested frame is grouped but never decoded", () => {
    const dbg = createInAppDebugger();
    // No identity: exactly the 3-arg call an embed makes today.
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes());

    expect(dbg.decodeTrusted("shop.dot")).toBe(false);
    expect(dbg.decodeTrusted()).toBe(false);

    const el = attached();
    const dispose = dbg.mount(el);

    // Payload-blind grouping needs no wire contract, so the op still lists.
    expect(el.querySelector(".ins-list")?.innerHTML).toContain(
      "account.getAccount",
    );
    // ...and the panel says the names may be wrong, as the standalone does.
    expect(el.querySelector(".ins-list")?.innerHTML).toContain(
      "declared no wire contract",
    );
    expect(el.querySelector(".ins-status")?.innerHTML).toContain(
      "wire identity unconfirmed",
    );

    openFirstOp(el);
    const html = el.querySelector(".ins-detail")?.innerHTML ?? "";
    // The value is NOT surfaced: the feeder's frame ids are not attested to mean
    // what this debugger's table says they mean.
    expect(html).not.toContain("alice.dot");
    expect(html).toContain("payload not shown");

    dispose();
    el.remove();
  });

  test("an attested frame decodes in the panel", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);

    expect(dbg.decodeTrusted("shop.dot")).toBe(true);

    const el = attached();
    const dispose = dbg.mount(el);
    openFirstOp(el);
    expect(el.querySelector(".ins-detail")?.innerHTML).toContain("alice.dot");
    expect(el.querySelector(".ins-list")?.innerHTML).not.toContain("⚠");

    dispose();
    el.remove();
  });

  test("a mismatched wire schema refuses decode and banners the drift", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), {
      ...ATTESTED,
      schema: "0000000000000000",
    });

    expect(dbg.decodeTrusted("shop.dot")).toBe(false);

    const el = attached();
    const dispose = dbg.mount(el);
    openFirstOp(el);
    expect(el.querySelector(".ins-detail")?.innerHTML).not.toContain(
      "alice.dot",
    );
    expect(el.querySelector(".ins-list")?.innerHTML).toContain(
      "differs from this debugger's",
    );
    expect(el.querySelector(".ins-status")?.innerHTML).toContain(
      "codec mismatch",
    );

    dispose();
    el.remove();
  });

  test("one mismatching frame marks the channel untrusted for good", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    expect(dbg.decodeTrusted("shop.dot")).toBe(true);
    // A later frame declaring a different codec version: sticky refusal.
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      { ...ATTESTED, codec: TRUAPI_CODEC_VERSION + 1 },
    );
    expect(dbg.decodeTrusted("shop.dot")).toBe(false);
  });

  test("a payload-blind mount needs no attestation (nothing decodes anyway)", () => {
    const dbg = createInAppDebugger({ decodeValues: false });
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes());
    expect(dbg.decodeTrusted("shop.dot")).toBe(true);
  });

  // --- one shared aggregate ----------------------------------------------

  test("the summary strip reports the whole shared aggregate", () => {
    // maxFramesPerTrace: 1 forces a `truncated` op; the malformed frame and the
    // unanswered subscription supply the other health tallies.
    const dbg = createInAppDebugger({ maxFramesPerTrace: 1 });
    dbg.handleFrame(
      "shop.dot",
      "out",
      frameBytes(W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start, [0], "p:9"),
      { ...ATTESTED, dropped: 3 },
    );
    dbg.handleFrame("shop.dot", "in", new Uint8Array([0xff, 0xff, 0xff]), ATTESTED);
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    // The shared roll-up over the same views is the reference: the strip must
    // agree with it rather than compute its own subset.
    const stats = computeTraceStats(
      dbg.session.traceEngine
        .traces()
        .map((t) => wireTraceToView(t, dbg.session.methodNames)),
    );
    expect(stats.malformed).toBe(1);
    expect(stats.truncated).toBe(1);
    expect(stats.subscriptions).toBe(1);
    expect(stats.liveSubscriptions).toBe(1);
    expect(stats.out).toBeGreaterThan(0);
    expect(stats.in).toBeGreaterThan(0);

    const el = container();
    const dispose = dbg.mount(el);
    const html = el.querySelector(".ins-summary")?.innerHTML ?? "";

    // Always-meaningful tiles, plus every exception counter that is NON-ZERO in
    // this fixture. Zero-valued exception counters render nothing (same rule as
    // the standalone's `warnTile`), so asserting their presence here would pin
    // the opposite behaviour.
    for (const label of [
      "ops",
      "frames",
      "data",
      "subs",
      "avg op",
      "malformed",
      "orphaned",
      "truncated",
      "dropped",
    ]) {
      expect(html).toContain(`class="k">${label}</span>`);
    }
    // Zero counters are omitted, not muted: `retryStorms`/`unpaired`/`evicted`
    // are 0 in this fixture. This is the half that keeps the rule honest - a
    // regression that re-rendered them would otherwise pass unnoticed.
    expect(stats.retryStorms).toBe(0);
    expect(html).not.toContain(`class="k">retry storms</span>`);
    expect(html).not.toContain(`class="k">unpaired</span>`);
    expect(html).not.toContain(`class="k">evicted</span>`);
    // The tallies the bespoke strip omitted entirely.
    expect(html).toContain(
      `<span class="n">${String(stats.malformed)}</span><span class="k">malformed</span>`,
    );
    expect(html).toContain(
      `<span class="n">${String(stats.truncated)}</span><span class="k">truncated</span>`,
    );
    expect(html).toContain(
      `<span class="n">3</span><span class="k">dropped</span>`,
    );
    // The in/out split and the observed maximum.
    expect(html).toContain(`${String(stats.out)}▶ ${String(stats.in)}◀`);
    expect(html).toContain("max ");

    dispose();
  });

  // --- retention ---------------------------------------------------------

  test("session retention caps reach the trace engine", () => {
    const session = createDebugSession({ maxTraces: 2 });
    for (const requestId of ["p:1", "p:2", "p:3"]) {
      session.handleEnvelope({
        channelId: "shop.dot",
        dir: "out",
        frame: frameBytes(W.ACCOUNT_GET_ACCOUNT.request, [0], requestId),
      });
    }
    expect(session.traceEngine.traces()).toHaveLength(2);
    expect(session.traceEngine.evictedTraces()).toBe(1);
  });

  test("the embed retains less than the standalone engine's default", () => {
    const dbg = createInAppDebugger();
    for (let i = 0; i < 200; i++) {
      dbg.handleFrame(
        "shop.dot",
        "out",
        frameBytes(W.ACCOUNT_GET_ACCOUNT.request, [0], `p:${String(i)}`),
        ATTESTED,
      );
    }
    // The engine default is 256 ops × 1 MiB of retained payload each; inside the
    // observed app's own tab that ceiling is the product's crash.
    expect(dbg.session.traceEngine.traces().length).toBeLessThanOrEqual(128);
    expect(dbg.session.traceEngine.evictedTraces()).toBeGreaterThan(0);
  });

  // --- containment -------------------------------------------------------

  test("a throwing session cannot break the product's frame path", () => {
    const dbg = createInAppDebugger();
    // The embed's tap runs in the host's own send/receive path, so a throw here
    // would surface to the product as a protocol failure.
    dbg.session.handleEnvelope = () => {
      throw new Error("boom");
    };
    expect(() => {
      dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    }).not.toThrow();
  });

  test("every injected rule is confined to the mount root", () => {
    const dbg = createInAppDebugger();
    const el = container();
    const dispose = dbg.mount(el);

    const css = el.querySelector("style")?.textContent ?? "";
    expect(css).not.toBe("");
    const leaked = ruleSelectors(css)
      .flatMap((list) => list.split(","))
      .map((s) => s.trim())
      .filter((s) => s !== "" && !s.startsWith(".td-inapp"));
    expect(leaked).toEqual([]);

    dispose();
  });

  test("the panel does not restyle the host application's own markup", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);

    const el = attached();
    const dispose = dbg.mount(el);

    // The host application's own debug panel uses the same `td-*` class names
    // (they are lifted from it), so a global rule restyles it.
    const hostPanel = win.document.createElement("div");
    hostPanel.className = "td-op";
    hostPanel.innerHTML = `<span class="td-op-meta">host panel</span>`;
    win.document.body.append(hostPanel);

    const inside = el.querySelector(".td-op-meta");
    const outside = hostPanel.querySelector(".td-op-meta");
    expect(inside).not.toBeNull();
    expect(outside).not.toBeNull();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- happy-dom element types
    const colorOf = (node: any): string =>
      win.getComputedStyle(node).color ?? "";
    // The panel's own rows are styled; the host's identically-classed markup is
    // untouched by anything the panel injected.
    expect(colorOf(inside)).not.toBe("");
    expect(colorOf(outside)).toBe("");
    expect(
      // eslint-disable-next-line @typescript-eslint/no-explicit-any -- happy-dom element types
      win.getComputedStyle(hostPanel as any).cursor ?? "",
    ).not.toBe("pointer");

    hostPanel.remove();
    dispose();
    el.remove();
  });

  // --- refresh cost ------------------------------------------------------

  test("an unchanged open op is not re-decoded on every refresh", () => {
    const dbg = createInAppDebugger();
    dbg.handleFrame("shop.dot", "out", accountGetRequestBytes(), ATTESTED);
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );

    const el = container();
    const dispose = dbg.mount(el);

    let decodes = 0;
    const real = dbg.session.decodedFrames.bind(dbg.session);
    dbg.session.decodedFrames = ((...args: Parameters<typeof real>) => {
      decodes += 1;
      return real(...args);
    }) as typeof dbg.session.decodedFrames;

    openFirstOp(el);
    expect(decodes).toBe(1);
    const rendered = el.querySelector(".ins-detail")?.innerHTML ?? "";

    // Three more render passes with nothing about the op changed: re-decoding and
    // re-hexing every frame here is ~50ms of blocked main thread per second, in
    // the product's own tab, for an identical result.
    const filter = el.querySelector<HTMLInputElement>(".ins-filter");
    for (let i = 0; i < 3; i++) {
      filter?.dispatchEvent(new win.Event("input") as unknown as Event);
    }
    expect(decodes).toBe(1);
    expect(el.querySelector(".ins-detail")?.innerHTML).toBe(rendered);

    // A new frame on the open op DOES refresh it.
    dbg.handleFrame(
      "shop.dot",
      "in",
      frameBytes(W.ACCOUNT_GET_ACCOUNT.response),
      ATTESTED,
    );
    filter?.dispatchEvent(new win.Event("input") as unknown as Event);
    expect(decodes).toBe(2);

    dispose();
  });

  // --- style precedence --------------------------------------------------

  test("a live-and-waiting op's meta reads waiting-amber, not live-green", () => {
    const dbg = createInAppDebugger();
    // A subscription start with nothing back: live (no stop) AND waiting
    // (orphaned opener). Both classes land on the same row.
    dbg.handleFrame(
      "shop.dot",
      "out",
      frameBytes(W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.start),
      ATTESTED,
    );
    const trace = dbg.session.traceEngine.traces()[0];
    expect(trace).toBeDefined();
    const view = wireTraceToView(trace!, dbg.session.methodNames);
    const rowHtml = renderOperationRow(view, { now: Date.now() + 5000 });
    expect(rowHtml).toContain("td-op-live");
    expect(rowHtml).toContain("td-op-waiting");

    const style = win.document.createElement("style");
    style.textContent = INSPECTOR_SHELL_CSS;
    const holder = win.document.createElement("div");
    holder.innerHTML = rowHtml;
    win.document.body.append(style, holder);

    // A stalled op must read as a problem: the waiting colour has to beat the
    // live one, whatever order the two rules are declared in.
    const meta = holder.querySelector(".td-op-meta");
    expect(meta).not.toBeNull();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any -- happy-dom element types
    expect(win.getComputedStyle(meta as any).color).toBe("#fbbf24");

    style.remove();
    holder.remove();
  });

  test("two ops whose ids would collide under a bare separator stay distinct", () => {
    // `opKey` joins (channel, request, generation) with a SPACE, and all three
    // components are sender-controlled and only length-clamped - `normalizeId`
    // filters no characters. Without the length prefixes, channel "a" + request
    // "b 0" and channel "a b" + request "0" both flatten to `a b 0 0`: one
    // selection key for two different ops, so clicking either highlighted BOTH
    // rows and the drill-down rendered whichever happened to sort first.
    //
    // Pins the render key and the click-path key TOGETHER. Mutating only one of
    // the two desynchronises them and is caught by any selection test; this is
    // the case that survives mutating both, which is the collision itself.
    const dbg = createInAppDebugger();
    dbg.handleFrame("a", "out", accountGetRequestBytes("b 0"), ATTESTED);
    dbg.handleFrame("a b", "out", accountGetRequestBytes("0"), ATTESTED);

    const el = attached();
    const dispose = dbg.mount(el);
    const rows = el.querySelectorAll<HTMLElement>(".td-op");
    expect(rows.length).toBe(2);

    rows[0]?.click();
    expect(el.querySelectorAll(".td-op.selected").length).toBe(1);

    dispose();
    el.remove();
  });
});

describe("waiting beats live in the cascade", () => {
  /**
   * An unanswered subscription carries BOTH `td-op-live` and `td-op-waiting`, so
   * the two rules collide on the same element. The amber wait has to win: green
   * says "healthy and streaming", which is the opposite of what an unanswered
   * opener means. Asserting the COMPUTED colour rather than source order is the
   * point - the previous rule pair was ordered wrongly at equal specificity, so
   * the amber was dead and no assertion on the stylesheet text would have caught
   * it.
   */
  test("a row that is both live and waiting computes amber, not green", () => {
    const win = new Window();
    const doc = win.document;
    const style = doc.createElement("style");
    style.textContent = `${INSPECTOR_SHELL_CSS}\n${INSPECTOR_LAYOUT_CSS}`;
    doc.head.appendChild(style);

    const row = doc.createElement("div");
    row.className = "td-op td-op-sub td-op-live td-op-waiting";
    const meta = doc.createElement("span");
    meta.className = "td-op-meta";
    row.appendChild(meta);
    doc.body.appendChild(row);

    // Amber, not the green a healthy live subscription gets.
    expect(win.getComputedStyle(meta as never).color).toBe("#fbbf24");
  });

  /**
   * The informational tiles (`infoStat` / `infoTile`) emit `ins-stat zero` with no
   * `warn`. While the dimming rule was written `.ins-stat.warn.zero`, that class
   * was inert for them: 0 and N computed the SAME colour, so the strip carried no
   * signal at all for the count it was added to surface - and at 0 it rendered as
   * bright as the headline metrics. Asserting the computed colour rather than the
   * stylesheet text is the point; the rule existed, it just never applied.
   */
  test("a neutral tile dims at zero and brightens at non-zero", () => {
    const win = new Window();
    const doc = win.document;
    const style = doc.createElement("style");
    style.textContent = `${INSPECTOR_SHELL_CSS}\n${INSPECTOR_LAYOUT_CSS}`;
    doc.head.appendChild(style);

    const colourOf = (cls: string): string => {
      const tile = doc.createElement("div");
      tile.className = cls;
      const n = doc.createElement("span");
      n.className = "n";
      tile.appendChild(n);
      doc.body.appendChild(tile);
      return win.getComputedStyle(n as never).color;
    };

    const neutralZero = colourOf("ins-stat zero");
    const neutralSome = colourOf("ins-stat");
    expect(neutralZero).not.toBe(neutralSome);
    // Dimmed to the same grey a zeroed warning uses.
    expect(neutralZero).toBe(colourOf("ins-stat warn zero"));
    // And a neutral count is never the alarm colour a real warning gets.
    expect(neutralSome).not.toBe(colourOf("ins-stat warn"));
  });

  test("a live row that is NOT waiting still computes green", () => {
    const win = new Window();
    const doc = win.document;
    const style = doc.createElement("style");
    style.textContent = `${INSPECTOR_SHELL_CSS}\n${INSPECTOR_LAYOUT_CSS}`;
    doc.head.appendChild(style);

    const row = doc.createElement("div");
    row.className = "td-op td-op-sub td-op-live";
    const meta = doc.createElement("span");
    meta.className = "td-op-meta";
    row.appendChild(meta);
    doc.body.appendChild(row);

    expect(win.getComputedStyle(meta as never).color).toBe("#4ade80");
  });
});

test("recordIdentity's per-frame verdict refuses a mismatched envelope version", () => {
  // The stamping is covered elsewhere; this pins the VERDICT. A matching schema
  // with a wrong `v` is "confirmed AND mismatched" and must not decode.
  //
  // Asserted through `session.frameDetail`, which is the STANDALONE mount's decode
  // entry - the in-app panel renders via `decodeTraceFrames` instead. So this pins
  // `recordIdentity`'s return value, not what this panel puts on screen; the two
  // agree today because both ultimately gate on the same verdict. The channel here
  // is mismatched, so `decodeTrusted` would refuse it as well - what isolates the
  // per-frame arm is the mutation that flips only that return.
  const d = createInAppDebugger();
  const bytes = accountGetRequestBytes();
  d.handleFrame("shop.dot", "out", bytes, {
    ...ATTESTED,
    v: (ATTESTED.v ?? 0) + 99,
  });
  const detail = d.session.frameDetail("p:1", 0, "shop.dot");
  expect(detail?.kind).toBe("bytes");
});

test("the per-frame verdict refuses a mismatch on a channel's SECOND frame", () => {
  // The existing-channel arm of `recordIdentity`, i.e. every frame after the first
  // on a channel - the common case, and a different return site from the one the
  // test above exercises. Same caveat as above: read through the standalone's
  // `frameDetail`, so it pins the verdict rather than this panel's render.
  const d = createInAppDebugger();
  const bytes = accountGetRequestBytes();
  // Frame 1 registers the channel, correctly attested.
  d.handleFrame("shop.dot", "out", bytes, ATTESTED);
  expect(d.session.frameDetail("p:1", 0, "shop.dot")?.kind).toBe("decoded");
  // Frame 2 on the SAME channel declares a wrong codec version.
  d.handleFrame("shop.dot", "out", bytes, {
    ...ATTESTED,
    codec: (ATTESTED.codec ?? 0) + 7,
  });
  // Both frames are openers for the same requestId, so the second rotates to a
  // new generation rather than joining the first trace.
  const second = d.session.frameDetail("p:1", 0, "shop.dot", 1);
  expect(second?.kind).toBe("bytes");
});
