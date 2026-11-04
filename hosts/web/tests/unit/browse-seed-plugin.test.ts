import { afterEach, describe, expect, test } from "bun:test";
import type { ConfigEnv, Plugin, UserConfig } from "vite";
import {
  SEED_ENV,
  type SeedReaders,
  browseSeed,
  fetchSeed,
  seedFiles,
} from "../../browse-seed-plugin.js";
import { Blocks, car } from "../../fixtures/archive/builders.js";
import {
  cidKey,
  cidToString,
  parseCidString,
  verifyBlock,
} from "../../src/products/archive/cid.js";
import type { SourceChoice } from "../../src/products/dotns.js";
import { SEED_MANIFEST_PATH } from "../../src/products/seed.js";
import { NETWORKS } from "../../src/platform/network-config.js";

const NETWORK = {
  ...NETWORKS.paseo,
  dotns: {
    ...NETWORKS.paseo.dotns,
    contentGateway: "https://gateway.test",
  },
};
const FETCHED_AT = new Date("2026-01-02T03:04:05.000Z");

/** An app archive, packed as the host's own build packs one, and its outer blocks. */
function archive() {
  const site = new Blocks();
  const bytes = car(
    site.tree({ "index.html": "<p>browse</p>", "app.js": "x".repeat(5000) }),
    site,
  );
  const outer = new Blocks();
  const root = outer.file(bytes, 1024);
  return { outer, cid: cidToString(root) };
}

/** A gateway that answers raw block requests from `blocks`, through `alter`, and records them. */
function fakeGateway(
  blocks: Blocks,
  alter: (data: Uint8Array) => Uint8Array = (data) => data,
) {
  const asked: string[] = [];
  const fetchFn = async (url: string) => {
    const parsed = new URL(url);
    asked.push(parsed.origin);
    expect(parsed.searchParams.get("format")).toBe("raw");
    const block = blocks.all.get(
      cidKey(parseCidString(parsed.pathname.slice("/ipfs/".length))),
    );
    if (block === undefined) return new Response("", { status: 404 });
    return new Response(alter(block.data) as BodyInit);
  };
  return { asked, fetchFn };
}

function readers(
  choice: SourceChoice,
  fetchFn: SeedReaders["fetch"],
): SeedReaders & { names: string[] } {
  const names: string[] = [];
  return {
    names,
    resolve: async (name) => {
      names.push(name);
      return choice;
    },
    fetch: fetchFn,
    now: () => FETCHED_AT,
  };
}

const found = (cid: string): SourceChoice => ({
  source: { record: "browse.paseo", cid, kind: "car" },
  skipped: [],
});

describe("fetching the Browse seed", () => {
  test("keeps every block of the archive, each checked against its CID, with a manifest", async () => {
    const { outer, cid } = archive();
    const gateway = fakeGateway(outer);
    const seed = await fetchSeed(
      "browse.paseo",
      NETWORK,
      readers(found(cid), gateway.fetchFn),
    );

    const blockBytes = [...outer.all.values()].reduce(
      (sum, { data }) => sum + data.length,
      0,
    );
    expect(seed.manifest).toEqual({
      record: "browse.paseo",
      cid,
      kind: "car",
      network: "paseo",
      fetchedAt: FETCHED_AT.toISOString(),
      bytes: blockBytes,
      blocks: expect.arrayContaining(
        [...outer.all.values()].map(({ cid }) => cidToString(cid)),
      ),
    });
    expect(seed.manifest.blocks).toHaveLength(outer.all.size);
    expect(new Set(gateway.asked)).toEqual(new Set(["https://gateway.test"]));

    const files = seedFiles(seed);
    expect([...files.keys()].sort()).toEqual(
      [
        SEED_MANIFEST_PATH,
        ...[...outer.all.values()].map(({ cid }) => `ipfs/${cidToString(cid)}`),
      ].sort(),
    );
    expect(JSON.parse(files.get(SEED_MANIFEST_PATH) as string)).toEqual(
      seed.manifest,
    );
    for (const [path, data] of files) {
      if (path === SEED_MANIFEST_PATH) continue;
      const blockCid = path.slice("ipfs/".length);
      expect({
        blockCid,
        verified: verifyBlock(parseCidString(blockCid), data as Uint8Array),
      }).toEqual({ blockCid, verified: true });
    }
  });

  test("refuses a gateway that sends a block other than the one asked for", async () => {
    const { outer, cid } = archive();
    const gateway = fakeGateway(outer, (data) =>
      data.length > 100 ? data.map((byte) => byte ^ 1) : data,
    );
    await expect(
      fetchSeed("browse.paseo", NETWORK, readers(found(cid), gateway.fetchFn)),
    ).rejects.toThrow("does not match its identifier");
  });

  test("refuses content larger than a seed may add", async () => {
    const { outer, cid } = archive();
    const gateway = fakeGateway(outer);
    await expect(
      fetchSeed(
        "browse.paseo",
        NETWORK,
        readers(found(cid), gateway.fetchFn),
        2048,
      ),
    ).rejects.toThrow("a seed may add");
  });

  test("fails when the name has nothing to open", async () => {
    await expect(
      fetchSeed(
        "browse.paseo",
        NETWORK,
        readers(
          { source: null, skipped: [] },
          fakeGateway(new Blocks()).fetchFn,
        ),
      ),
    ).rejects.toThrow("browse.paseo has no app or site on Paseo Next v2");
  });
});

/** Run the plugin's hooks as a build or dev server would, collecting what it emits. */
async function run(
  plugin: Plugin,
  env: ConfigEnv,
  config: UserConfig = {},
): Promise<{ define: unknown; emitted: Map<string, unknown> }> {
  const configure = plugin.config as (
    config: UserConfig,
    env: ConfigEnv,
  ) => Promise<UserConfig>;
  const result = await configure(config, env);
  const emitted = new Map<string, unknown>();
  const generate = plugin.generateBundle as unknown as (this: {
    emitFile(file: { fileName: string; source: unknown }): void;
  }) => void;
  generate.call({
    emitFile: (file) => void emitted.set(file.fileName, file.source),
  });
  return {
    define: JSON.parse(String(result.define?.__CONTENT_SEED__)),
    emitted,
  };
}

const BUILD: ConfigEnv = { command: "build", mode: "production" };
const DEV: ConfigEnv = { command: "serve", mode: "development" };

describe("the Browse seed plugin", () => {
  const setting = process.env[SEED_ENV];
  afterEach(() => {
    if (setting === undefined) delete process.env[SEED_ENV];
    else process.env[SEED_ENV] = setting;
  });

  function plugin() {
    const { outer, cid } = archive();
    const gateway = fakeGateway(outer);
    const fake = readers(found(cid), gateway.fetchFn);
    return { cid, outer, fake, plugin: browseSeed(NETWORK, fake) };
  }

  test("seeds a build by default, defining the seed and writing its blocks", async () => {
    delete process.env[SEED_ENV];
    const { cid, outer, fake, plugin: seeding } = plugin();
    const { define, emitted } = await run(seeding, BUILD);
    expect(fake.names).toEqual(["browse.paseo"]);
    expect(define).toEqual({ record: "browse.paseo", cid, kind: "car" });
    expect(emitted.size).toBe(outer.all.size + 1);
    expect(emitted.has(`ipfs/${cid}`)).toBe(true);
  });

  test("builds without a seed when told to, and asks nothing", async () => {
    process.env[SEED_ENV] = "0";
    const { fake, plugin: seeding } = plugin();
    expect(await run(seeding, BUILD)).toEqual({
      define: null,
      emitted: new Map(),
    });
    expect(fake.names).toEqual([]);
  });

  test("leaves a dev server and a preview without a seed unless asked", async () => {
    delete process.env[SEED_ENV];
    const { fake, plugin: seeding } = plugin();
    expect((await run(seeding, DEV)).define).toBeNull();
    process.env[SEED_ENV] = "1";
    expect((await run(seeding, { ...DEV, isPreview: true })).define).toBeNull();
    expect(fake.names).toEqual([]);
    expect((await run(seeding, DEV)).define).not.toBeNull();
  });

  test("fails the build when the seed cannot be fetched", async () => {
    delete process.env[SEED_ENV];
    const seeding = browseSeed(
      NETWORK,
      readers(found(archive().cid), fakeGateway(new Blocks()).fetchFn),
    );
    await expect(run(seeding, BUILD)).rejects.toThrow(
      `Set ${SEED_ENV}=0 to build without it.`,
    );
  });

  // A failed lookup must not leave the seed of an earlier run in the build.
  test("keeps no seed when the name cannot be resolved", async () => {
    delete process.env[SEED_ENV];
    const { cid, outer } = archive();
    let reachable = true;
    const seeding = browseSeed(NETWORK, {
      ...readers(found(cid), fakeGateway(outer).fetchFn),
      resolve: async () => {
        if (!reachable)
          throw new Error(
            "no answer from https://rpc.test for childstate_getStorage after 15s",
          );
        return found(cid);
      },
    });
    expect((await run(seeding, BUILD)).define).not.toBeNull();
    reachable = false;
    await expect(run(seeding, BUILD)).rejects.toThrow(
      "Could not seed browse.paseo from Paseo Next v2: no answer from https://rpc.test",
    );
    const emitted: string[] = [];
    (
      seeding.generateBundle as unknown as (this: {
        emitFile(file: { fileName: string }): void;
      }) => void
    ).call({ emitFile: (file) => void emitted.push(file.fileName) });
    expect(emitted).toEqual([]);
  });

  // The blocks are read at the origin's root, so a build under a path would ship blocks no load reads.
  test("builds under another base path without a seed, and refuses one asked for", async () => {
    delete process.env[SEED_ENV];
    const { fake, plugin: seeding } = plugin();
    expect(await run(seeding, BUILD, { base: "/repo/" })).toEqual({
      define: null,
      emitted: new Map(),
    });
    expect(fake.names).toEqual([]);
    process.env[SEED_ENV] = "1";
    await expect(run(seeding, BUILD, { base: "/repo/" })).rejects.toThrow(
      'needs base "/"',
    );
  });
});
