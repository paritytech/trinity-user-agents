import { describe, expect, test } from "bun:test";
import { Blocks, car } from "../../../fixtures/archive/builders.js";
import {
  cidKey,
  cidToString,
  parseCidString,
} from "../../../src/products/archive/cid.js";
import {
  ARCHIVE_LIMITS,
  type FetchLike,
  gatewayBlocks,
  loadArchive,
} from "../../../src/products/archive/gateway.js";
import {
  type ContentSeed,
  blockGateway,
  seededKind,
} from "../../../src/products/seed.js";

const ORIGIN = "https://host.test";
const GATEWAY = "https://gateway.test";

function archive(page: string) {
  const site = new Blocks();
  const outer = new Blocks();
  const root = outer.file(car(site.tree({ "index.html": page }), site), 256);
  return { outer, cid: cidToString(root) };
}

/**
 * The host's origin serving the seeded blocks, as a build writes them, and a
 * gateway serving every product. Both record the origin of each request.
 */
function network(seeded: Blocks, published: Blocks) {
  const asked: string[] = [];
  const fetchFn = async (url: string) => {
    const parsed = new URL(url);
    asked.push(parsed.origin);
    const blocks = parsed.origin === ORIGIN ? seeded : published;
    const block = blocks.all.get(
      cidKey(parseCidString(parsed.pathname.slice("/ipfs/".length))),
    );
    return block === undefined
      ? new Response("", { status: 404 })
      : new Response(block.data as BodyInit);
  };
  return { asked, fetchFn };
}

/** Load `cid` as the loader does, from wherever {@link blockGateway} says. */
async function load(cid: string, seed: ContentSeed, fetchFn: FetchLike) {
  const from = blockGateway(cid, GATEWAY, ORIGIN, seed);
  const { files } = await loadArchive(
    cid,
    gatewayBlocks(from, ARCHIVE_LIMITS, fetchFn),
  );
  return new TextDecoder().decode(files.get("index.html"));
}

describe("the seed a build ships", () => {
  const browse = archive("<p>seeded browse</p>");
  const seed: ContentSeed = {
    record: "browse.paseo",
    cid: browse.cid,
    kind: "car",
  };

  test("is read from the host's origin when the chain points at its CID", async () => {
    const { asked, fetchFn } = network(browse.outer, browse.outer);
    expect(await load(browse.cid, seed, fetchFn)).toBe("<p>seeded browse</p>");
    expect(new Set(asked)).toEqual(new Set([ORIGIN]));
    expect(seededKind(browse.cid, seed)).toBe("car");
  });

  // A republished name points elsewhere: the seed must not stand in for it.
  test("is not used when the chain points at another CID", async () => {
    const republished = archive("<p>new browse</p>");
    const { asked, fetchFn } = network(browse.outer, republished.outer);
    expect(await load(republished.cid, seed, fetchFn)).toBe(
      "<p>new browse</p>",
    );
    expect(new Set(asked)).toEqual(new Set([GATEWAY]));
    expect(seededKind(republished.cid, seed)).toBeNull();
  });

  test("is still checked block by block, so altered files on the origin are refused", async () => {
    const altered = new Blocks();
    for (const [key, block] of browse.outer.all)
      altered.all.set(key, {
        cid: block.cid,
        data:
          block.data.length > 100
            ? block.data.map((byte) => byte ^ 1)
            : block.data,
      });
    const { fetchFn } = network(altered, browse.outer);
    await expect(load(browse.cid, seed, fetchFn)).rejects.toThrow(
      "does not match its identifier",
    );
  });

  test("is absent from a build or dev server without one", () => {
    expect(blockGateway(browse.cid, GATEWAY, ORIGIN, null)).toBe(GATEWAY);
    expect(seededKind(browse.cid, null)).toBeNull();
  });
});
