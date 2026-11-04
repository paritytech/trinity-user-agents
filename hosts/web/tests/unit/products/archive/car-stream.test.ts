import { describe, expect, test } from "bun:test";
import { Blocks, car } from "../../../../fixtures/archive/builders.js";
import {
  type Cid,
  RAW,
  cidKey,
  cidToString,
  parseCidString,
} from "../../../../src/products/archive/cid.js";
import { readCarStream } from "../../../../src/products/archive/car.js";
import {
  ARCHIVE_LIMITS,
  type ArchiveLimits,
  carBlocks,
  carCounts,
  carSkip,
  gatewayBlocks,
  loadArchive,
  loadSite,
  rawCounts,
} from "../../../../src/products/archive/gateway.js";
import { transportOf } from "../../../../src/products/load-observer.js";

const CAR_TYPE = "application/vnd.ipld.car; version=1; order=dfs; dups=n";

/** Bytes that differ for every `seed`, so no two files share a block. */
const pattern = (length: number, seed: number) =>
  new Uint8Array(length).map((_, index) => (index * 7 + seed * 31) % 251);

/** A site of distinct chunked files, and the CAR a gateway would send for it. */
function site() {
  const blocks = new Blocks();
  const files: Record<string, Uint8Array> = {
    "index.html": new TextEncoder().encode("<p>ok</p>"),
  };
  for (let index = 0; index < 4; index += 1)
    files[`assets/${index}.bin`] = pattern(3000 + index, index + 1);
  const root = blocks.tree(files);
  return { blocks, root, files, bytes: car(root, blocks) };
}

/** The keys of `blocks` in the order a CAR of them lists them. */
const order = (blocks: Blocks) => [...blocks.all.keys()];

/** Where the section of the `count`th block of `blocks` ends in their CAR. */
function sectionEnd(root: Cid, blocks: Blocks, count: number): number {
  const prefix = new Blocks();
  for (const entry of [...blocks.all.values()].slice(0, count))
    prefix.all.set(cidKey(entry.cid), entry);
  return car(root, prefix).length;
}

interface CarAnswer {
  /** Bytes sent before the stream ends, closes or hangs. */
  cut?: number;
  /** After `cut`, keep the stream open with nothing more to send. */
  hang?: boolean;
  /** Bytes per chunk. */
  chunk?: number;
  /** Wait before each chunk. */
  delayMs?: number;
  /** Wait before the first chunk. */
  startMs?: number;
  status?: number;
  type?: string;
  /** Reject the request as a network failure would. */
  unreachable?: boolean;
}

/**
 * A gateway over `blocks` that answers `?format=car` with `bytes` as `answer`
 * says, and `?format=raw` with the stored block. It records every request.
 */
function fakeGateway(
  blocks: Blocks,
  bytes: Uint8Array,
  answer: CarAnswer = {},
  rawDelayMs = 0,
) {
  const raw: string[] = [];
  const cars: AbortSignal[] = [];
  const accepts: (string | null)[] = [];
  let sent = 0;
  const fetchFn = async (url: string, init?: RequestInit) => {
    const parsed = new URL(url);
    const signal = init!.signal!;
    if (parsed.searchParams.get("format") === "raw") {
      const key = cidKey(
        parseCidString(parsed.pathname.slice("/ipfs/".length)),
      );
      raw.push(key);
      if (rawDelayMs > 0) await after(rawDelayMs);
      return new Response(blocks.all.get(key)!.data as BodyInit);
    }
    expect(parsed.searchParams.get("format")).toBe("car");
    cars.push(signal);
    accepts.push(new Headers(init?.headers).get("accept"));
    if (answer.unreachable) throw new TypeError("connection reset");
    const cut = answer.cut ?? bytes.length;
    const chunk = answer.chunk ?? 7;
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        signal.addEventListener("abort", () => controller.error(signal.reason));
      },
      async pull(controller) {
        if (sent === 0 && answer.startMs !== undefined)
          await after(answer.startMs);
        if (answer.delayMs !== undefined) await after(answer.delayMs);
        if (sent >= cut) {
          if (!answer.hang) controller.close();
          return new Promise(() => {});
        }
        const end = Math.min(sent + chunk, cut);
        controller.enqueue(bytes.slice(sent, end));
        sent = end;
      },
    });
    return new Response(body, {
      status: answer.status ?? 200,
      headers: { "content-type": answer.type ?? CAR_TYPE },
    });
  };
  return { raw, cars, accepts, fetchFn, sent: () => sent };
}

function reader(
  root: Cid,
  fetchFn: (url: string, init?: RequestInit) => Promise<Response>,
  limits: ArchiveLimits = ARCHIVE_LIMITS,
) {
  return carBlocks(
    root,
    "https://gateway.test",
    limits,
    gatewayBlocks("https://gateway.test", limits, fetchFn),
    fetchFn,
  );
}

/** As {@link reader}, with what the stream and the block reads did counted. */
function countedReader(
  root: Cid,
  fetchFn: (url: string, init?: RequestInit) => Promise<Response>,
  limits: ArchiveLimits = ARCHIVE_LIMITS,
) {
  const raw = rawCounts();
  const car = carCounts();
  const getBlock = carBlocks(
    root,
    "https://gateway.test",
    limits,
    gatewayBlocks("https://gateway.test", limits, fetchFn, raw),
    fetchFn,
    car,
  );
  const transport = () =>
    transportOf({ outcome: "fetched", totalMs: 0, raw, car });
  return { getBlock, raw, car, transport };
}

const sorted = (keys: string[]) => [...keys].sort();
const after = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe("reading content from one CAR stream", () => {
  test("loads a site from the stream alone, with every file in order", async () => {
    const { blocks, root, files, bytes } = site();
    const gateway = fakeGateway(blocks, bytes);
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn),
    );
    expect(new Map([...loaded.files].sort())).toEqual(
      new Map(Object.entries(files).sort()),
    );
    expect(gateway.cars.length).toBe(1);
    expect(gateway.accepts).toEqual(["application/vnd.ipld.car"]);
    expect(gateway.raw).toEqual([]);
  });

  test("loads an app archive from the stream of its file's DAG", async () => {
    const inner = new Blocks();
    const innerRoot = inner.tree({ "index.html": "<p>app</p>" });
    const outer = new Blocks();
    const outerRoot = outer.file(car(innerRoot, inner), 64);
    const gateway = fakeGateway(outer, car(outerRoot, outer), { chunk: 1 });
    const loaded = await loadArchive(
      cidToString(outerRoot),
      reader(outerRoot, gateway.fetchFn),
    );
    expect(new TextDecoder().decode(loaded.files.get("index.html"))).toBe(
      "<p>app</p>",
    );
    expect(gateway.raw).toEqual([]);
  });

  test("reads only the blocks a stream cut short did not bring", async () => {
    const { blocks, root, files, bytes } = site();
    const kept = 5;
    const gateway = fakeGateway(blocks, bytes, {
      cut: sectionEnd(root, blocks, kept) + 9,
    });
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn),
    );
    expect(loaded.files.size).toBe(Object.keys(files).length);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks).slice(kept)));
  });

  test("refuses a block that does not match, and reads it and the rest once each", async () => {
    const { blocks, root, bytes } = site();
    const forged = 3;
    const tampered = bytes.slice();
    tampered[sectionEnd(root, blocks, forged + 1) - 1] ^= 1;
    const gateway = fakeGateway(blocks, tampered);
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn),
    );
    expect(loaded.files.size).toBe(5);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks).slice(forged)));
    expect(gateway.cars[0].aborted).toBe(true);
  });

  test("stops waiting on a stream that brings nothing for a window", async () => {
    const { blocks, root, bytes } = site();
    const kept = 4;
    const gateway = fakeGateway(blocks, bytes, {
      cut: sectionEnd(root, blocks, kept) + 3,
      hang: true,
    });
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, {
        ...ARCHIVE_LIMITS,
        carWindowMs: 30,
        carWindowBytes: 1,
        carHedgeMs: 60_000,
      }),
    );
    expect(loaded.files.size).toBe(5);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks).slice(kept)));
  });

  test("drops a stream that keeps sending too slowly, and reads each missing block once", async () => {
    const { blocks, root, bytes } = site();
    // Seven bytes every 5 ms bring every block in about ten seconds.
    const gateway = fakeGateway(blocks, bytes, { delayMs: 5 });
    const started = performance.now();
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, {
        ...ARCHIVE_LIMITS,
        carWindowMs: 40,
        carWindowBytes: 1000,
        carHedgeMs: 60_000,
      }),
    );
    expect(loaded.files.size).toBe(5);
    expect(performance.now() - started).toBeLessThan(1000);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(gateway.raw.length).toBeGreaterThan(0);
    expect(gateway.raw.length).toBe(new Set(gateway.raw).size);
  });

  test("stops waiting for a stream that keeps its pace but is slow, and loads while it still runs", async () => {
    const { blocks, root, files, bytes } = site();
    // Seven bytes every 5 ms pass a pace of one byte per 40 ms, and would
    // bring every block in about ten seconds.
    const gateway = fakeGateway(blocks, bytes, { delayMs: 5 });
    const started = performance.now();
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, {
        ...ARCHIVE_LIMITS,
        carWindowMs: 40,
        carWindowBytes: 1,
        carHedgeMs: 50,
      }),
    );
    expect(new Map([...loaded.files].sort())).toEqual(
      new Map(Object.entries(files).sort()),
    );
    expect(performance.now() - started).toBeLessThan(1000);
    expect(gateway.sent()).toBeLessThan(bytes.length);
    expect(gateway.raw.length).toBeGreaterThan(0);
    expect(gateway.raw.length).toBe(new Set(gateway.raw).size);
  });

  test("uses the blocks a stream brings after reads stopped waiting for it", async () => {
    const { blocks, root, bytes } = site();
    // The root's read stops waiting at 20 ms and asks for it alone, which
    // takes 200 ms; the stream brings every block at 100 ms.
    const gateway = fakeGateway(
      blocks,
      bytes,
      { startMs: 100, chunk: bytes.length },
      200,
    );
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, { ...ARCHIVE_LIMITS, carHedgeMs: 20 }),
    );
    expect(loaded.files.size).toBe(5);
    expect(gateway.raw).toEqual([cidKey(root)]);
  });

  test("drops a stream whose header names another root, and reads every block by itself", async () => {
    const { blocks, root } = site();
    const otherSite = new Blocks();
    const otherRoot = otherSite.tree({ "index.html": "<p>other</p>" });
    // The other CAR stays open after its blocks, so only its header can end it.
    const gateway = fakeGateway(blocks, car(otherRoot, otherSite), {
      hang: true,
    });
    const started = performance.now();
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, { ...ARCHIVE_LIMITS, carHedgeMs: 60_000 }),
    );
    expect(loaded.files.size).toBe(5);
    expect(performance.now() - started).toBeLessThan(1000);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks)));
  });

  test("holds at most an archive's bytes from the stream, and reads the rest block by block", async () => {
    const { blocks, root, bytes } = site();
    const firstThree = [...blocks.all.values()]
      .slice(0, 3)
      .reduce((sum, { data }) => sum + data.length, 0);
    const gateway = fakeGateway(blocks, bytes);
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, {
        ...ARCHIVE_LIMITS,
        maxBytes: firstThree,
      }),
    );
    expect(loaded.files.size).toBe(5);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks).slice(3)));
  });

  test("reads block by block at once when the CAR request fails, and does not ask for it again", async () => {
    const { blocks, root, bytes } = site();
    for (const answer of [
      { status: 404 },
      { status: 503 },
      { type: "application/vnd.ipld.raw" },
      { type: "text/html" },
      { unreachable: true },
    ]) {
      const gateway = fakeGateway(blocks, bytes, answer);
      const started = performance.now();
      const loaded = await loadSite(
        cidToString(root),
        reader(root, gateway.fetchFn),
      );
      expect(loaded.files.size).toBe(5);
      expect(performance.now() - started).toBeLessThan(1000);
      expect(gateway.cars.length).toBe(1);
      expect(sorted(gateway.raw)).toEqual(sorted(order(blocks)));
    }
  });

  test("ends the stream at its own limits and reads the rest block by block", async () => {
    const { blocks, root, bytes } = site();
    const gateway = fakeGateway(blocks, bytes);
    const loaded = await loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn, { ...ARCHIVE_LIMITS, maxBlocks: 3 }),
    );
    expect(loaded.files.size).toBe(5);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(sorted(gateway.raw)).toEqual(sorted(order(blocks).slice(3)));
  });

  test("refuses content over the byte limit, and stops the stream and the block reads", async () => {
    const { blocks, root, bytes } = site();
    const total = [...blocks.all.values()].reduce(
      (sum, { data }) => sum + data.length,
      0,
    );
    const gateway = fakeGateway(blocks, bytes);
    const limits = { ...ARCHIVE_LIMITS, maxTotalBytes: total - 1 };
    await expect(
      loadSite(
        cidToString(root),
        reader(root, gateway.fetchFn, limits),
        limits,
      ),
    ).rejects.toThrow("larger than this host accepts");
    expect(gateway.cars[0].aborted).toBe(true);
    expect(gateway.raw.length).toBeLessThanOrEqual(1);
  });

  test("charges each block once against the byte limit, whichever way it came", async () => {
    const { blocks, root, bytes } = site();
    const total = [...blocks.all.values()].reduce(
      (sum, { data }) => sum + data.length,
      0,
    );
    const limits = { ...ARCHIVE_LIMITS, maxTotalBytes: total };
    for (const cut of [bytes.length, sectionEnd(root, blocks, 6) + 2]) {
      const gateway = fakeGateway(blocks, bytes, { cut });
      const loaded = await loadSite(
        cidToString(root),
        reader(root, gateway.fetchFn, limits),
        limits,
      );
      expect(loaded.files.size).toBe(5);
      expect(gateway.raw.length).toBe(
        cut === bytes.length ? 0 : blocks.all.size - 6,
      );
    }
  });

  test("stops the stream and reads nothing block by block when the load is cancelled", async () => {
    const { blocks, root, bytes } = site();
    const gateway = fakeGateway(blocks, bytes, {
      cut: sectionEnd(root, blocks, 2) + 1,
      hang: true,
    });
    const controller = new AbortController();
    const loading = loadSite(
      cidToString(root),
      reader(root, gateway.fetchFn),
      ARCHIVE_LIMITS,
      controller.signal,
    );
    await after(20);
    controller.abort();
    await expect(loading).rejects.toThrow("cancelled");
    await after(5);
    expect(gateway.cars[0].aborted).toBe(true);
    expect(gateway.raw).toEqual([]);
  });
});

describe("counting what a load did", () => {
  test("counts a load the stream answered alone as CAR", async () => {
    const { blocks, root, bytes } = site();
    const gateway = fakeGateway(blocks, bytes);
    const counted = countedReader(root, gateway.fetchFn);
    await loadSite(cidToString(root), counted.getBlock);
    expect(counted.car).toMatchObject({
      requested: true,
      status: 200,
      bytes: bytes.length,
      blocks: blocks.all.size,
      end: "complete",
      hedged: false,
      fromCar: blocks.all.size,
      fallback: 0,
    });
    expect(counted.car.firstByteMs).toBeGreaterThanOrEqual(0);
    expect(counted.car.ms).toBeGreaterThanOrEqual(counted.car.firstByteMs!);
    expect(counted.raw).toEqual(rawCounts());
    expect(counted.transport()).toBe("car");
  });

  test("counts the reads the hedge sent block by block as CAR+raw, while the stream still runs", async () => {
    const { blocks, root, bytes } = site();
    // As above: the root's read stops waiting at 20 ms and is fetched alone;
    // the stream brings every block at 100 ms.
    const gateway = fakeGateway(
      blocks,
      bytes,
      { startMs: 100, chunk: bytes.length, hang: true },
      200,
    );
    const counted = countedReader(root, gateway.fetchFn, {
      ...ARCHIVE_LIMITS,
      carHedgeMs: 20,
    });
    const load = new AbortController();
    await loadSite(
      cidToString(root),
      counted.getBlock,
      ARCHIVE_LIMITS,
      load.signal,
    );
    // The root arrived both ways: the stream brought every block, and the
    // root's read had fetched it alone too.
    expect(counted.car).toMatchObject({
      hedged: true,
      blocks: blocks.all.size,
      fromCar: blocks.all.size - 1,
      fallback: 1,
    });
    expect(counted.car.end).toBeUndefined();
    expect(counted.raw).toEqual({
      ...rawCounts(),
      requests: 1,
      blocks: 1,
      bytes: blocks.all.get(cidKey(root))!.data.length,
    });
    expect(counted.transport()).toBe("car+raw");
    load.abort();
  });

  test("counts a refused stream and a load read block by block as raw", async () => {
    const { blocks, root, bytes } = site();
    const gateway = fakeGateway(blocks, bytes, { status: 404 });
    const counted = countedReader(root, gateway.fetchFn);
    await loadSite(cidToString(root), counted.getBlock);
    expect(counted.car).toMatchObject({
      requested: true,
      status: 404,
      end: "refused",
      bytes: 0,
      blocks: 0,
      fromCar: 0,
      fallback: blocks.all.size,
    });
    expect(counted.car.reason).toContain("404");
    expect(counted.raw).toMatchObject({
      requests: blocks.all.size,
      blocks: blocks.all.size,
      retries: 0,
    });
    expect(counted.transport()).toBe("raw");
  });

  test("says why a stream ended early", async () => {
    const { blocks, root, bytes } = site();
    const tampered = bytes.slice();
    tampered[sectionEnd(root, blocks, 4) - 1] ^= 1;
    const forged = countedReader(root, fakeGateway(blocks, tampered).fetchFn);
    await loadSite(cidToString(root), forged.getBlock);
    expect(forged.car.end).toBe("failed");
    expect(forged.car.reason).toContain("does not match");

    const stalled = countedReader(
      root,
      fakeGateway(blocks, bytes, {
        cut: sectionEnd(root, blocks, 2),
        hang: true,
      }).fetchFn,
      {
        ...ARCHIVE_LIMITS,
        carWindowMs: 30,
        carWindowBytes: 1,
        carHedgeMs: 60_000,
      },
    );
    await loadSite(cidToString(root), stalled.getBlock);
    expect(stalled.car.end).toBe("slow");
    expect(stalled.car.reason).toBeUndefined();

    const cancelled = countedReader(
      root,
      fakeGateway(blocks, bytes, {
        cut: sectionEnd(root, blocks, 2) + 1,
        hang: true,
      }).fetchFn,
    );
    const controller = new AbortController();
    const loading = loadSite(
      cidToString(root),
      cancelled.getBlock,
      ARCHIVE_LIMITS,
      controller.signal,
    );
    await after(20);
    controller.abort();
    await expect(loading).rejects.toThrow("cancelled");
    await after(5);
    expect(cancelled.car.end).toBe("cancelled");
  });

  test("keeps what a failed load counted", async () => {
    const { blocks, root, bytes } = site();
    const missing = order(blocks)[2];
    const gateway = fakeGateway(blocks, bytes, { status: 404 });
    const counted = countedReader(root, async (url, init) => {
      const key = new URL(url).pathname.slice("/ipfs/".length);
      if (cidKey(parseCidString(key)) === missing)
        return new Response("", { status: 404 });
      return gateway.fetchFn(url, init);
    });
    await expect(loadSite(cidToString(root), counted.getBlock)).rejects.toThrow(
      "404",
    );
    expect(counted.car.end).toBe("refused");
    expect(counted.raw.requests).toBeGreaterThan(0);
    expect(counted.raw.blocks).toBeLessThan(blocks.all.size);
  });
});

describe("whether a load asks for a CAR stream", () => {
  // The host's own origin is a static server: a CAR request there gets a raw
  // block back and is refused, one wasted request per load.
  test("asks a gateway, never the host's own origin, and nothing when it is off", () => {
    const origin = "https://host.test";
    expect([
      carSkip("https://gateway.test", origin),
      carSkip(origin, origin),
      carSkip("https://gateway.test", origin, false),
    ]).toEqual([null, "host-origin", "off"]);
  });
});

describe("counting block-by-block reads", () => {
  test("counts a retry, a read that joins an open request, and one kept in memory", async () => {
    const { blocks, root } = site();
    let calls = 0;
    const counts = rawCounts();
    const getBlock = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      async () => {
        calls += 1;
        if (calls === 1) return new Response("", { status: 502 });
        return new Response(blocks.all.get(cidKey(root))!.data as BodyInit);
      },
      counts,
    );
    await Promise.all([getBlock(root), getBlock(root)]);
    await getBlock(root);
    expect(counts).toEqual({
      requests: 2,
      blocks: 1,
      bytes: blocks.all.get(cidKey(root))!.data.length,
      retries: 1,
      cacheHits: 1,
      joined: 1,
    });
  });
});

describe("reading a CAR stream", () => {
  async function* chunked(bytes: Uint8Array, size: number) {
    for (let at = 0; at < bytes.length; at += size)
      yield bytes.slice(at, at + size);
  }

  test("yields every block, however the stream is split", async () => {
    const { blocks, root, bytes } = site();
    for (const size of [1, 7, 4096, bytes.length]) {
      const read: string[] = [];
      for await (const block of readCarStream(
        chunked(bytes, size),
        root,
        ARCHIVE_LIMITS,
      ))
        read.push(cidKey(block.cid));
      expect(read).toEqual(order(blocks));
    }
  });

  test("refuses a section longer than a block may be before buffering it", async () => {
    const blocks = new Blocks();
    const root = blocks.add(RAW, pattern(1000, 1));
    const bytes = car(root, blocks);
    let pulled = 0;
    async function* counted() {
      for await (const chunk of chunked(bytes, 64)) {
        pulled += 1;
        yield chunk;
      }
    }
    const read = async () => {
      for await (const _ of readCarStream(counted(), root, {
        ...ARCHIVE_LIMITS,
        maxBlockBytes: 16,
      }));
    };
    await expect(read()).rejects.toThrow("larger than this host accepts");
    // The header and the section's length fit in two chunks; its block
    // would need sixteen more.
    expect(pulled).toBeLessThanOrEqual(2);
  });
});
