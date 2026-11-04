import { describe, expect, test } from "bun:test";
import {
  LOAD_SUMMARY_LIMITS,
  PRODUCT_LOAD,
  type ProductLoadSummary,
  type ReadTiming,
  fileStats,
  openingEvent,
  readLoadMessage,
  timeReads,
  transportOf,
} from "../../../src/products/load-observer.js";
import { carCounts, rawCounts } from "../../../src/products/archive/gateway.js";
import type { Cid } from "../../../src/products/archive/cid.js";
import { loaderUrl } from "../../../src/products/loader/mount.js";

const full: ProductLoadSummary = {
  outcome: "fetched",
  totalMs: 812.5,
  workerMs: 40,
  loadMs: 700,
  fetchMs: 620,
  reads: 12,
  storeMs: 30,
  files: { count: 3, bytes: 4096, largest: { path: "app.js", bytes: 4000 } },
  raw: { ...rawCounts(), requests: 3, blocks: 2, bytes: 900, retries: 1 },
  car: {
    ...carCounts(),
    requested: true,
    status: 200,
    firstByteMs: 80,
    bytes: 5000,
    blocks: 10,
    ms: 2100,
    end: "complete",
    hedged: true,
    fromCar: 10,
    fallback: 2,
  },
};

const message = (summary: unknown) => ({ type: PRODUCT_LOAD, summary });

describe("the loader's summary as the host reads it", () => {
  test("keeps every field of a well-formed summary", () => {
    expect(readLoadMessage(message(full))).toEqual(full);
  });

  test("keeps why a load asked for no CAR stream, and only a reason it knows", () => {
    const skipped = {
      outcome: "fetched",
      totalMs: 3,
      carSkipped: "host-origin",
    };
    expect(readLoadMessage(message(skipped))).toEqual(
      skipped as ProductLoadSummary,
    );
    expect(
      readLoadMessage(message({ ...skipped, carSkipped: "bored" })),
    ).toEqual({ outcome: "fetched", totalMs: 3 });
  });

  test("is no summary without its type, outcome or total time", () => {
    for (const data of [
      null,
      "text",
      { type: "truapi-sandbox", summary: full },
      message(null),
      message({ ...full, outcome: "maybe" }),
      message({ ...full, totalMs: -1 }),
      message({ ...full, totalMs: Number.NaN }),
      message({ outcome: "fetched" }),
    ])
      expect({ data, read: readLoadMessage(data) }).toEqual({
        data,
        read: null,
      });
  });

  // A frame decides what it posts: the host keeps a bounded, typed copy and
  // never a field it does not know.
  test("bounds its text, and drops fields that do not read or it does not know", () => {
    const read = readLoadMessage(
      message({
        outcome: "failed",
        totalMs: 5,
        reason: "x".repeat(1000),
        product: "two.paseo",
        bytes: new Uint8Array(4),
        workerMs: "fast",
        fetchMs: Infinity,
        reads: 1.5,
        files: {
          count: 1,
          bytes: 10,
          largest: { path: "p".repeat(1000), bytes: 10 },
        },
        raw: { ...rawCounts(), requests: -1 },
        car: { ...carCounts(), requested: true, end: "exploded", status: 4000 },
      }),
    );
    expect(read).toEqual({
      outcome: "failed",
      totalMs: 5,
      reason: "x".repeat(LOAD_SUMMARY_LIMITS.reason),
      files: {
        count: 1,
        bytes: 10,
        largest: { path: "p".repeat(LOAD_SUMMARY_LIMITS.path), bytes: 10 },
      },
      car: { ...carCounts(), requested: true },
    });
  });
});

describe("a load's transport", () => {
  const summaryOf = (
    outcome: ProductLoadSummary["outcome"],
    fromCar: number,
    requests: number,
  ): ProductLoadSummary => ({
    outcome,
    totalMs: 1,
    raw: { ...rawCounts(), requests },
    car: { ...carCounts(), requested: true, fromCar },
  });

  test("follows what answered the reads", () => {
    expect([
      transportOf(summaryOf("fetched", 5, 0)),
      transportOf(summaryOf("fetched", 5, 2)),
      transportOf(summaryOf("fetched", 0, 7)),
      transportOf(summaryOf("cached", 0, 0)),
      transportOf(summaryOf("failed", 0, 0)),
      transportOf({ outcome: "failed", totalMs: 1 }),
    ]).toEqual(["car", "car+raw", "raw", "cache", "none", "none"]);
  });
});

describe("fileStats", () => {
  test("counts the files, their bytes and the largest", () => {
    expect(
      fileStats(
        new Map([
          ["index.html", new Uint8Array(10)],
          ["app.js", new Uint8Array(300)],
          ["a.css", new Uint8Array(20)],
        ]),
      ),
    ).toEqual({
      count: 3,
      bytes: 330,
      largest: { path: "app.js", bytes: 300 },
    });
    expect(fileStats(new Map())).toEqual({ count: 0, bytes: 0 });
  });
});

describe("timeReads", () => {
  const after = (ms: number) =>
    new Promise<void>((resolve) => setTimeout(resolve, ms));

  // Reads overlap; the wait is the time any was open, not their sum.
  test("counts overlapping reads' wait once", async () => {
    const timing: ReadTiming = { reads: 0, waitMs: 0 };
    const read = timeReads(
      async () => {
        await after(60);
        return new Uint8Array(1);
      },
      performance.now(),
      timing,
    );
    const cid = {} as Cid;
    await Promise.all([read(cid), read(cid), read(cid)]);
    await after(40);
    await read(cid);
    expect(timing.reads).toBe(4);
    expect(timing.waitMs).toBeGreaterThanOrEqual(110);
    expect(timing.waitMs).toBeLessThan(170);
    expect(timing.lastBlockMs!).toBeGreaterThanOrEqual(150);
  });
});

describe("what opening a name tells the Products view", () => {
  const ORIGIN = "https://host.test";
  const GATEWAY = "https://gateway.test";
  /**
   * A name's target, with the loader URL built as the host builds it: a
   * build's seed gives the loader this host's origin as its gateway.
   */
  const target = (cid: string, gateway: string) => ({
    address: "browse.paseo",
    via: "name" as const,
    cid,
    kind: "car" as const,
    mount: `/product/w/browse/${cid}/`,
    record: "app.browse.paseo",
    resolveMs: 42,
    url: loaderUrl({
      base: "/",
      origin: ORIGIN,
      scope: `/product/w/browse/${cid}/`,
      gateway,
      kind: "car",
      start: "/",
    }),
  });

  // The row names where the loader read the blocks: the host's origin when
  // the host serves a name's blocks itself, the gateway otherwise.
  test("names the gateway the loader was given, and a seed when it is this host", () => {
    expect(openingEvent(target("bafyseeded", ORIGIN), "App", ORIGIN)).toEqual({
      type: "opening",
      load: expect.any(Number),
      mount: "/product/w/browse/bafyseeded/",
      product: "browse.paseo",
      execution: "App",
      record: "app.browse.paseo",
      cid: "bafyseeded",
      kind: "car",
      gateway: ORIGIN,
      source: "seed",
      resolveMs: 42,
    });
    expect(
      openingEvent(target("bafyother", GATEWAY), "App", ORIGIN),
    ).toMatchObject({
      gateway: GATEWAY,
      source: "gateway",
    });
  });

  test("is nothing for a product opened by URL", () => {
    expect(
      openingEvent(
        {
          address: "http://localhost:4000/",
          via: "url",
          url: new URL("http://localhost:4000/"),
        },
        "App",
        ORIGIN,
      ),
    ).toBeNull();
  });
});
