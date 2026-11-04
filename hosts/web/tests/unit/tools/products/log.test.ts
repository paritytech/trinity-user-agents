import { describe, expect, test } from "bun:test";
import type { ProductLoadEvent } from "../../../../src/products/load-observer.js";
import { MAX_LOADS, ProductLoadLog } from "../../../../tools/products/log.js";
import { transportText } from "../../../../tools/products/view.js";

let loads = 0;

const opening = (
  mount: string,
  product = "one.paseo",
): Extract<ProductLoadEvent, { type: "opening" }> => ({
  type: "opening",
  load: (loads += 1),
  mount,
  product,
  execution: "App",
  record: `app.${product}`,
  cid: "bafy-content",
  kind: "car",
  gateway: "https://gateway.test",
  source: "gateway",
  resolveMs: 120,
});

const loaded = (
  { load, mount }: { load: number; mount: string },
  totalMs = 300,
): ProductLoadEvent => ({
  type: "loaded",
  load,
  mount,
  summary: { outcome: "fetched", totalMs },
});

const closed = (
  { load, mount }: { load: number; mount: string },
  reason?: string,
): ProductLoadEvent => ({
  type: "closed",
  load,
  mount,
  ...(reason === undefined ? {} : { reason }),
});

/** Each row's mount and where it stands. */
const rows = (log: ProductLoadLog) =>
  log.list().map((record) => ({
    mount: record.mount,
    outcome:
      record.summary?.outcome ??
      (record.closeReason !== undefined
        ? "failed"
        : record.closedAt !== undefined
          ? "closed"
          : "waiting"),
  }));

describe("the Products view's log", () => {
  test("joins a loader's summary to its own load only", () => {
    let at = 1000;
    const log = new ProductLoadLog(() => (at += 1));
    const one = opening("/m/one/");
    log.record(one);
    log.record(loaded({ load: one.load, mount: "/m/elsewhere/" }, 9));
    log.record(loaded({ load: one.load + 100, mount: one.mount }, 9));
    log.record(loaded(one));
    const { type: _type, ...facts } = one;
    expect(log.list()).toEqual([
      {
        ...facts,
        openedAt: 1001,
        summary: { outcome: "fetched", totalMs: 300 },
        summaryAt: 1002,
      },
    ]);
  });

  // One row per mount: opening it again is a new load. The earlier frame's
  // late report or close says nothing about it.
  test("replaces a mount's load when it is opened again, and ignores the earlier frame", () => {
    const log = new ProductLoadLog(() => 0);
    const first = opening("/m/one/");
    log.record(first);
    log.record(loaded(first));
    log.record(opening("/m/two/", "two.paseo"));
    const again = opening("/m/one/");
    log.record(again);
    log.record(closed(first));
    log.record(loaded(first, 5));
    expect(rows(log)).toEqual([
      { mount: "/m/one/", outcome: "waiting" },
      { mount: "/m/two/", outcome: "waiting" },
    ]);
    expect(log.list()[0].load).toBe(again.load);
  });

  // A row must not wait forever: a frame closed first, or one that never
  // opened, says so.
  test("marks a load closed before its loader reported, and failed when its frame never opened", () => {
    const log = new ProductLoadLog(() => 7);
    const quick = opening("/m/quick/");
    const broken = opening("/m/broken/");
    const done = opening("/m/done/");
    for (const event of [quick, broken, done]) log.record(event);
    log.record(closed(quick));
    log.record(closed(broken, "no provider"));
    log.record(loaded(done));
    log.record(closed(done));
    expect(rows(log)).toEqual([
      { mount: "/m/done/", outcome: "fetched" },
      { mount: "/m/broken/", outcome: "failed" },
      { mount: "/m/quick/", outcome: "closed" },
    ]);
    expect(log.list()[1]).toMatchObject({
      closedAt: 7,
      closeReason: "no provider",
    });
  });

  test(`keeps at most ${MAX_LOADS} loads and counts the ones it dropped`, () => {
    const log = new ProductLoadLog(() => 0);
    for (let index = 0; index < MAX_LOADS + 3; index += 1)
      log.record(opening(`/m/${index}/`));
    const mounts = log.list().map((record) => record.mount);
    expect(mounts.length).toBe(MAX_LOADS);
    expect(mounts[0]).toBe(`/m/${MAX_LOADS + 2}/`);
    expect(mounts.at(-1)).toBe("/m/3/");
    expect(log.dropped()).toBe(3);
  });

  test("starts again for a new session, and tells its listeners", () => {
    const log = new ProductLoadLog(() => 0);
    let changes = 0;
    log.onChange(() => (changes += 1));
    expect(log.startSession("paseo:in:w1")).toBe(true);
    const one = opening("/m/one/");
    log.record(one);
    log.record(closed(one));
    expect(log.startSession("paseo:in:w1")).toBe(false);
    expect(log.list().length).toBe(1);
    expect(log.startSession("paseo:out:")).toBe(true);
    expect({ loads: log.list(), dropped: log.dropped(), changes }).toEqual({
      loads: [],
      dropped: 0,
      changes: 4,
    });
  });

  // Around a session change the host closes the product in front and opens
  // it again, and at boot it restores the tab's product, in no fixed order
  // with the session's start. A frame still open keeps its row either way.
  test("keeps a load whose frame is open when the session starts, whichever comes first", () => {
    // The session starts after the product is open again.
    const late = new ProductLoadLog(() => 0);
    late.startSession("paseo:in:w1");
    const before = opening("/m/w1/one/");
    late.record(before);
    late.record(loaded(before));
    late.record(closed(before));
    const reopened = opening("/m/w2/one/");
    late.record(reopened);
    late.startSession("paseo:in:w2");
    late.record(loaded(reopened));
    expect(rows(late)).toEqual([{ mount: "/m/w2/one/", outcome: "fetched" }]);

    // The session starts first: the product opens into it.
    const early = new ProductLoadLog(() => 0);
    early.startSession("paseo:in:w2");
    const restored = opening("/m/w2/one/");
    early.record(restored);
    expect(rows(early)).toEqual([{ mount: "/m/w2/one/", outcome: "waiting" }]);
  });

  // An earlier session's worker still running when the session starts is
  // stopped right after: its row goes when its frame closes.
  test("drops a load kept over a session start once its frame closes", () => {
    const log = new ProductLoadLog(() => 0);
    log.startSession("paseo:in:w1");
    const worker = opening("/m/w1/chat/");
    log.record(worker);
    log.record(loaded(worker));
    log.startSession("paseo:in:w2");
    expect(rows(log)).toEqual([{ mount: "/m/w1/chat/", outcome: "fetched" }]);
    log.record(closed(worker));
    expect(rows(log)).toEqual([]);
  });
});

describe("a load's transport, as the Products view writes it", () => {
  // A seed and the host's own archives are read from this origin: the row
  // says so beside how the blocks arrived, and a cache hit fetched nothing.
  test("names the host origin beside the transport", () => {
    expect([
      transportText("car", "gateway"),
      transportText("raw", "seed"),
      transportText("car+raw", "host"),
      transportText("cache", "seed"),
      transportText("none", "host"),
    ]).toEqual(["CAR", "raw · seed", "CAR+raw · host", "cache", "none"]);
  });
});
