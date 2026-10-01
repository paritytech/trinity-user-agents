import { describe, expect, test } from "bun:test";
import { Blocks, car, dagPb } from "./fixtures.js";
import {
  DAG_PB,
  RAW,
  cidFor,
  cidKey,
  cidToString,
  parseCidString,
  verifyBlock,
} from "./cid.js";
import { parseCar } from "./car.js";
import {
  ARCHIVE_LIMITS,
  gatewayBlocks,
  loadArchive,
  loadSite,
} from "./gateway.js";
import { readFile, unpackDirectory } from "./unixfs.js";

const text = (value: string) => new TextEncoder().encode(value);
const decode = (bytes: Uint8Array | undefined) =>
  bytes === undefined ? undefined : new TextDecoder().decode(bytes);
const fromBlocks =
  (blocks: Blocks) => async (cid: Parameters<typeof cidKey>[0]) => {
    const found = blocks.all.get(cidKey(cid));
    if (found === undefined) throw new Error("missing block");
    return found.data;
  };

/** Publish `files` the way an executable is: a CAR of the site as a chunked file. */
function publish(files: Record<string, string | Uint8Array>) {
  const site = new Blocks();
  const root = site.tree(files);
  const archive = car(root, site);
  const outer = new Blocks();
  const outerRoot = outer.file(archive, 512);
  return { site, root, archive, outer, outerRoot };
}

describe("CIDs", () => {
  // A real chunked-file root read from the Paseo Bulletin gateway, with the
  // raw leaf it links to.
  const chainCid =
    "bafybeiaffszja3e2n7sccwljzyr5mookybefof2t65vrlzj6psq7f3iye4";
  const chainBlock = Uint8Array.from(
    "122c0a2401551220 4a2cd56d3b829f50e66240dbecdc20c8e296f34c8a330be0f8f0e902ddcfb2ef1200 18eae4280a0a080218eae42820eae428"
      .replaceAll(" ", "")
      .match(/../g)!
      .map((pair) => Number.parseInt(pair, 16)),
  );

  test("round-trips through text", () => {
    const cid = cidFor(RAW, text("hello"));
    expect(parseCidString(cidToString(cid))).toEqual(cid);
  });

  test("matches a block published on chain", () => {
    expect(verifyBlock(parseCidString(chainCid), chainBlock)).toBe(true);
    expect(verifyBlock(parseCidString(chainCid), chainBlock.slice(1))).toBe(
      false,
    );
  });

  test("refuses a hash it cannot compute instead of passing it", () => {
    const cid = { ...cidFor(RAW, text("x")), hashCode: 0xb220 };
    expect(() => verifyBlock(cid, text("x"))).toThrow("unsupported hash");
  });
});

describe("loading an archive", () => {
  test("unpacks nested directories and chunked files, byte for byte", async () => {
    const big = new Uint8Array(5000).map((_, index) => index % 251);
    const { outer, outerRoot } = publish({
      "index.html": "<!doctype html><title>x</title>",
      "assets/app.js": "console.log(1)",
      "assets/deep/data.bin": big,
    });
    const loaded = await loadArchive(cidToString(outerRoot), fromBlocks(outer));
    expect([...loaded.files.keys()].sort()).toEqual([
      "assets/app.js",
      "assets/deep/data.bin",
      "index.html",
    ]);
    expect(loaded.files.get("assets/deep/data.bin")).toEqual(big);
    expect(decode(loaded.files.get("index.html"))).toBe(
      "<!doctype html><title>x</title>",
    );
  });

  test("refuses an archive whose block was altered", async () => {
    const { site, root } = publish({ "index.html": "<p>ok</p>" });
    const bytes = car(root, site);
    bytes[bytes.length - 1] ^= 1;
    expect(() => parseCar(bytes, ARCHIVE_LIMITS)).toThrow(
      "does not match its identifier",
    );
  });

  test("refuses a chunk of the archive file that the record's CID does not cover", async () => {
    const { outer, outerRoot } = publish({ "index.html": "<p>ok</p>" });
    const tampered = new Blocks();
    for (const { cid, data } of outer.all.values()) {
      const copy = data.slice();
      if (cid.codec === RAW) copy[0] ^= 1;
      tampered.all.set(cidKey(cid), { cid, data: copy });
    }
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      async (url: string) => {
        const requested = parseCidString(
          new URL(String(url)).pathname.slice("/ipfs/".length),
        );
        return new Response(
          tampered.all.get(cidKey(requested))!.data as BodyInit,
        );
      },
    );
    await expect(loadArchive(cidToString(outerRoot), gateway)).rejects.toThrow(
      "does not match its identifier",
    );
  });

  test("fetches raw blocks from the gateway", async () => {
    const { outer, outerRoot } = publish({ "index.html": "<p>ok</p>" });
    const urls: string[] = [];
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      async (url: string) => {
        urls.push(String(url));
        const requested = parseCidString(
          new URL(String(url)).pathname.slice("/ipfs/".length),
        );
        return new Response(outer.all.get(cidKey(requested))!.data as BodyInit);
      },
    );
    await loadArchive(cidToString(outerRoot), gateway);
    expect(urls.every((url) => url.endsWith("?format=raw"))).toBe(true);
    expect(urls.length).toBeGreaterThan(1);
  });

  test("refuses a gateway block over the size cap", async () => {
    const { outerRoot } = publish({ "index.html": "<p>ok</p>" });
    const gateway = gatewayBlocks(
      "https://gateway.test",
      { maxBlockBytes: 8 },
      async () => new Response(new Uint8Array(64)),
    );
    await expect(loadArchive(cidToString(outerRoot), gateway)).rejects.toThrow(
      "larger than",
    );
  });

  test("refuses an archive without index.html", async () => {
    const { outer, outerRoot } = publish({ "other.html": "x" });
    await expect(
      loadArchive(cidToString(outerRoot), fromBlocks(outer)),
    ).rejects.toThrow("no index.html");
  });

  test("refuses an archive that does not name exactly one root", async () => {
    const site = new Blocks();
    const root = site.tree({ "index.html": "a" });
    for (const roots of [[], [root, root]]) {
      const outer = new Blocks();
      const outerRoot = outer.file(car(roots, site), 512);
      await expect(
        loadArchive(cidToString(outerRoot), fromBlocks(outer)),
      ).rejects.toThrow("exactly one root");
    }
  });
});

describe("unpacking a directory", () => {
  const limits = ARCHIVE_LIMITS;

  test("refuses names that would leave the site", async () => {
    for (const name of ["..", ".", "a/b", "a\\b", "", "a\0b"]) {
      const blocks = new Blocks();
      const file = blocks.file(text("x"));
      const root = blocks.add(
        DAG_PB,
        dagPb({ type: 1 }, [{ cid: file, name }]),
      );
      await expect(
        unpackDirectory(root, fromBlocks(blocks), limits),
      ).rejects.toThrow("unsafe file name");
    }
  });

  test("refuses two entries with one name", async () => {
    const blocks = new Blocks();
    const file = blocks.file(text("x"));
    const root = blocks.add(
      DAG_PB,
      dagPb({ type: 1 }, [
        { cid: file, name: "a" },
        { cid: file, name: "a" },
      ]),
    );
    await expect(
      unpackDirectory(root, fromBlocks(blocks), limits),
    ).rejects.toThrow("duplicate name");
  });

  test("refuses a sharded directory, a symlink and a metadata node instead of skipping them", async () => {
    for (const [type, message] of [
      [5, "sharded"],
      [4, "expected a file"],
      [3, "expected a file"],
    ] as const) {
      const blocks = new Blocks();
      const node = blocks.add(DAG_PB, dagPb({ type, data: text("target") }));
      const root = blocks.directory({ "index.html": node });
      await expect(
        unpackDirectory(root, fromBlocks(blocks), limits),
      ).rejects.toThrow(message);
    }
  });

  test("counts a shared block every time it is used", async () => {
    const blocks = new Blocks();
    const shared = blocks.file(new Uint8Array(600), 1024);
    const root = blocks.directory({ a: shared, b: shared, c: shared });
    await expect(
      unpackDirectory(root, fromBlocks(blocks), {
        ...limits,
        maxTotalBytes: 1500,
      }),
    ).rejects.toThrow("larger than this host accepts");
  });

  test("refuses too many files, too much nesting and a long path", async () => {
    const files = new Blocks();
    const file = files.file(text("x"));
    const flat = files.directory({ a: file, b: file, c: file });
    await expect(
      unpackDirectory(flat, fromBlocks(files), { ...limits, maxFiles: 2 }),
    ).rejects.toThrow("more files");

    const nested = new Blocks();
    let dir = nested.directory({ leaf: nested.file(text("x")) });
    for (let level = 0; level < 5; level += 1)
      dir = nested.directory({ d: dir });
    await expect(
      unpackDirectory(dir, fromBlocks(nested), {
        ...limits,
        maxDirectoryDepth: 3,
      }),
    ).rejects.toThrow("nested too deeply");

    const long = new Blocks();
    const leaf = long.directory({ ["x".repeat(200)]: long.file(text("x")) });
    await expect(
      unpackDirectory(
        long.directory({ ["y".repeat(200)]: leaf }),
        fromBlocks(long),
        { ...limits, maxPathBytes: 300 },
      ),
    ).rejects.toThrow("too long");
  });

  test("refuses a file whose recorded sizes disagree with its chunks", async () => {
    const blocks = new Blocks();
    const leaf = blocks.add(RAW, text("abcd"));
    const wrongChunk = blocks.add(
      DAG_PB,
      dagPb({ type: 2, fileSize: 4, blockSizes: [3] }, [
        { cid: leaf, name: "" },
      ]),
    );
    await expect(
      readFile(wrongChunk, fromBlocks(blocks), limits),
    ).rejects.toThrow("chunk is not the size");
    const wrongTotal = blocks.add(
      DAG_PB,
      dagPb({ type: 2, fileSize: 9, blockSizes: [4] }, [
        { cid: leaf, name: "" },
      ]),
    );
    await expect(
      readFile(wrongTotal, fromBlocks(blocks), limits),
    ).rejects.toThrow("not the size its node records");
  });

  test("reports a block the archive lacks", async () => {
    const site = new Blocks();
    const root = site.tree({ "index.html": "x" });
    const empty = new Blocks();
    await expect(
      unpackDirectory(root, fromBlocks(empty), limits),
    ).rejects.toThrow("missing block");
  });
});

/** A block source that records every identifier it is asked for. */
function counted(blocks: Blocks) {
  const asked: string[] = [];
  const getBlock = async (cid: Parameters<typeof cidKey>[0]) => {
    asked.push(cidKey(cid));
    return fromBlocks(blocks)(cid);
  };
  return { asked, getBlock };
}

/** A fetch that serves `blocks` as a gateway would, and counts requests. */
function gatewayOver(blocks: Blocks) {
  const requested: string[] = [];
  const fetchFn = async (url: string) => {
    const cid = parseCidString(new URL(url).pathname.slice("/ipfs/".length));
    requested.push(cidKey(cid));
    return new Response(blocks.all.get(cidKey(cid))!.data as BodyInit);
  };
  return { requested, fetchFn };
}

describe("bounding the work of a load", () => {
  test("charges a directory that links one empty directory many times", async () => {
    const blocks = new Blocks();
    const empty = blocks.directory({});
    const root = blocks.directory(
      Object.fromEntries(
        Array.from({ length: 500 }, (_, index) => [`d${index}`, empty]),
      ),
    );
    const { asked, getBlock } = counted(blocks);
    await expect(
      unpackDirectory(root, getBlock, { ...ARCHIVE_LIMITS, maxNodes: 50 }),
    ).rejects.toThrow("more blocks than this host accepts");
    expect(asked.length).toBe(50);
  });

  test("charges empty raw chunks that spend no bytes", async () => {
    const blocks = new Blocks();
    const empty = blocks.add(RAW, new Uint8Array(0));
    const file = blocks.add(
      DAG_PB,
      dagPb(
        { type: 2 },
        Array.from({ length: 500 }, () => ({ cid: empty, name: "" })),
      ),
    );
    const { asked, getBlock } = counted(blocks);
    await expect(
      readFile(file, getBlock, { ...ARCHIVE_LIMITS, maxNodes: 20 }),
    ).rejects.toThrow("more blocks than this host accepts");
    expect(asked.length).toBe(20);
  });

  test("stops a DAG that doubles at every level", async () => {
    const blocks = new Blocks();
    let level = blocks.directory({});
    for (let depth = 0; depth < 20; depth += 1)
      level = blocks.directory({ a: level, b: level });
    const { asked, getBlock } = counted(blocks);
    await expect(
      unpackDirectory(level, getBlock, {
        ...ARCHIVE_LIMITS,
        maxNodes: 100,
        maxFiles: 10,
      }),
    ).rejects.toThrow("more blocks than this host accepts");
    expect(asked.length).toBeLessThanOrEqual(100);
  });

  test("charges block bytes even when they hold no file data", async () => {
    const blocks = new Blocks();
    const empty = blocks.directory({});
    const root = blocks.directory({ a: empty, b: empty, c: empty });
    await expect(
      unpackDirectory(root, fromBlocks(blocks), {
        ...ARCHIVE_LIMITS,
        maxTotalBytes: 20,
      }),
    ).rejects.toThrow("larger than this host accepts");
  });

  test("bounds the archive file and the site inside it separately", async () => {
    const inner = new Blocks();
    const emptyDirectory = inner.directory({});
    const root = inner.directory(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => [
          `d${index}`,
          emptyDirectory,
        ]),
      ),
    );
    const archive = car(root, inner);

    const chunked = new Blocks();
    const chunkedRoot = chunked.file(archive, 64);
    const outerReads = counted(chunked);
    await expect(
      loadArchive(cidToString(chunkedRoot), outerReads.getBlock, {
        ...ARCHIVE_LIMITS,
        maxNodes: 15,
      }),
    ).rejects.toThrow("more blocks than this host accepts");
    expect(outerReads.asked.length).toBe(15);

    const whole = new Blocks();
    const wholeRoot = whole.file(archive, archive.length);
    await expect(
      loadArchive(cidToString(wholeRoot), fromBlocks(whole), {
        ...ARCHIVE_LIMITS,
        maxNodes: 50,
      }),
    ).rejects.toThrow("more blocks than this host accepts");
  });

  test("refuses an archive with too many sections", () => {
    const blocks = new Blocks();
    for (let index = 0; index < 10; index += 1)
      blocks.add(RAW, text(`block ${index}`));
    const root = blocks.add(RAW, text("root"));
    expect(() =>
      parseCar(car(root, blocks), { ...ARCHIVE_LIMITS, maxBlocks: 5 }),
    ).toThrow("more blocks than this host accepts");
  });

  test("fetches every block of a site once, however often it is linked", async () => {
    const blocks = new Blocks();
    const shared = blocks.file(text("shared"), 3);
    const site = blocks.directory({
      "index.html": blocks.file(text("<p>ok</p>")),
      a: shared,
      b: shared,
      c: blocks.directory({ d: shared, e: shared }),
    });
    const { requested, fetchFn } = gatewayOver(blocks);
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      fetchFn,
    );
    const loaded = await loadSite(cidToString(site), gateway);
    expect(loaded.files.size).toBe(5);
    expect(requested.length).toBe(new Set(requested).size);
  });

  test("keeps verifying a repeated block that was served from memory", async () => {
    const blocks = new Blocks();
    const shared = blocks.file(text("shared"), 3);
    const site = blocks.directory({
      "index.html": shared,
      a: shared,
    });
    const { fetchFn } = gatewayOver(blocks);
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      fetchFn,
    );
    const loaded = await loadSite(cidToString(site), gateway);
    expect(decode(loaded.files.get("a"))).toBe("shared");
  });

  test("gives up when the deadline passes while the gateway is slow", async () => {
    const blocks = new Blocks();
    const root = blocks.tree({ "index.html": "x" });
    const fetchFn = (_url: string, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) =>
        init?.signal?.addEventListener("abort", () =>
          reject(init.signal!.reason),
        ),
      );
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      fetchFn,
    );
    await expect(
      loadSite(cidToString(root), gateway, {
        ...ARCHIVE_LIMITS,
        deadlineMs: 20,
      }),
    ).rejects.toThrow("took too long");
  });

  test("gives up between blocks when the deadline passes on a source that ignores the signal", async () => {
    const blocks = new Blocks();
    const root = blocks.tree({
      "index.html": "x",
      "a.js": "y",
      "b.js": "z",
    });
    const slow = async (cid: Parameters<typeof cidKey>[0]) => {
      await new Promise((resolve) => setTimeout(resolve, 15));
      return fromBlocks(blocks)(cid);
    };
    await expect(
      loadSite(cidToString(root), slow, { ...ARCHIVE_LIMITS, deadlineMs: 20 }),
    ).rejects.toThrow("took too long");
  });

  test("stops when the caller cancels, and returns no partial files", async () => {
    const blocks = new Blocks();
    const root = blocks.tree({ "index.html": "x", "a.js": "y" });
    const controller = new AbortController();
    let reads = 0;
    const getBlock = async (cid: Parameters<typeof cidKey>[0]) => {
      reads += 1;
      if (reads === 3) controller.abort();
      return fromBlocks(blocks)(cid);
    };
    await expect(
      loadSite(cidToString(root), getBlock, ARCHIVE_LIMITS, controller.signal),
    ).rejects.toThrow("cancelled");
    expect(reads).toBe(3);
  });

  test("still refuses a block that does not match after the limits pass", async () => {
    const blocks = new Blocks();
    const root = blocks.tree({ "index.html": "x" });
    const gateway = gatewayBlocks(
      "https://gateway.test",
      ARCHIVE_LIMITS,
      async () => new Response(text("forged")),
    );
    await expect(loadSite(cidToString(root), gateway)).rejects.toThrow(
      "does not match its identifier",
    );
  });
});
