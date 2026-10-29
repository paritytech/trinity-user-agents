import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, readdir, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type DotnsTransport, DotnsRevert } from "./dotns.js";
import {
  type Fetch,
  type Model,
  encodeLink,
  generateSecret,
  publicKey,
  publish,
} from "./index.js";
import { type ProjectOptions, add, list, remove, update } from "./project.js";
import { createStackReader } from "./reader.js";

const VECTORS = new URL(
  "../../../../rust/crates/trinity-stack/vectors/",
  import.meta.url,
);
const cases = JSON.parse(
  await readFile(new URL("cases.json", VECTORS), "utf8"),
);
const controller = cases.dotns[0];

const URL_ = "https://stack.example/.well-known/trinity-stack.json";
const MODEL: Model = {
  id: "local",
  name: "Local",
  network: "local",
  tld: "local",
  chains: [
    {
      role: "Relay",
      genesisHash: `0x${"86".repeat(32)}`,
      rpc: ["ws://127.0.0.1:9944"],
    },
    {
      role: "AssetHub",
      genesisHash: `0x${"bb".repeat(32)}`,
      paraId: 1000,
      rpc: ["ws://127.0.0.1:9945"],
    },
  ],
  services: [],
};
const secret = generateSecret();
const link = encodeLink({ url: URL_, key: publicKey(secret) });

async function at(seconds: number, generation = "g1", model = MODEL) {
  const { document } = await publish(model, {
    generation,
    secret,
    now: new Date(seconds * 1000),
  });
  return document.slice();
}

/** Asset Hub as the controller vector has it, at block `number`. */
const chainAt = (number: number) => (): DotnsTransport => ({
  at: async () => ({ ...controller.at, number }),
  storage: async () => controller.storage,
  call: async (to, data) => {
    const answer = controller.calls[`${to}:${data}`];
    if (answer === null) throw new DotnsRevert("execution reverted");
    return answer;
  },
});

let dir: string;
let served: Uint8Array<ArrayBuffer> | undefined;
const serve: Fetch = async () =>
  served ? new Response(served.slice()) : new Response(null, { status: 503 });
const options = (block = 1): ProjectOptions => ({
  dir,
  fetch: serve,
  transport: chainAt(block),
});
const files = async () => (await readdir(dir)).sort();

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "trinity-stack-"));
  served = await at(100);
});
afterEach(() => rm(dir, { recursive: true, force: true }));

describe("trinity-stack add and update", () => {
  test("add writes the signed copy, its dotNS record and a module a reader runs from", async () => {
    const { changed } = await add("local", link, undefined, options());
    expect(changed.sort()).toEqual([
      "index.d.ts",
      "index.js",
      "local.dotns.json",
      "local.json",
      "stacks.json",
    ]);
    expect(new Uint8Array(await readFile(join(dir, "local.json")))).toEqual(
      served!,
    );
    const { stacks } = await import(join(dir, "index.js"));
    const reader = createStackReader({
      bundled: stacks,
      refresh: "never",
      fetch: () => Promise.reject(new Error("no fetch under never")),
      transport: () => {
        throw new Error("no lookup while the record matches");
      },
    });
    expect((await reader.get(stacks.local)).descriptor.sequence).toBe(100);
    expect((await reader.dotns(stacks.local))?.dispatcher).toBe(
      controller.expect.dotns.dispatcher,
    );
    expect(await readFile(join(dir, "index.d.ts"), "utf8")).toContain(
      'readonly "local": Bundled;',
    );
  });

  test("update writes nothing when only the block moved, and exits on a change otherwise", async () => {
    await add("local", link, undefined, options());
    expect(await update([], options(2))).toEqual({ changed: [], failures: [] });
    served = await at(200);
    expect((await update([], options(3))).changed.sort()).toEqual([
      "index.js",
      "local.json",
    ]);
    served = await at(300, "g2");
    expect((await update([], options(4))).changed.sort()).toEqual([
      "index.js",
      "local.dotns.json",
      "local.json",
    ]);
    const [stack] = await list({ dir });
    expect(stack.descriptor?.generation).toBe("g2");
    expect(stack.dotns?.at.number).toBe(4);
  });

  test("update refuses a lower sequence, an unreachable stack and an unsigned one, naming the stack", async () => {
    await add("local", link, undefined, options());
    const before = await readFile(join(dir, "local.json"));
    served = await at(50);
    expect(await update([], options())).toEqual({
      changed: [],
      failures: [
        "local: https://stack.example/.well-known/trinity-stack.json serves a lower sequence than the committed copy",
      ],
    });
    served = undefined;
    expect((await update(["local"], options())).failures).toEqual([
      "local: https://stack.example/.well-known/trinity-stack.json is unreachable",
    ]);
    expect(await readFile(join(dir, "local.json"))).toEqual(before);
    await expect(add("open", URL_, undefined, options())).rejects.toThrow(
      "must be signed",
    );
    await expect(update(["missing"], options())).rejects.toThrow(
      "no stack named missing",
    );
  });

  test("a stack whose Asset Hub is down keeps its copies, not a new descriptor beside an old record", async () => {
    await add("local", link, undefined, options());
    const before = await readFile(join(dir, "local.json"));
    served = await at(200, "g2");
    const down: ProjectOptions = {
      ...options(),
      transport: () => ({
        ...chainAt(1)(),
        at: () => Promise.reject(new Error("Asset Hub is down")),
      }),
    };
    const outcome = await update([], down);
    expect(outcome).toEqual({
      changed: [],
      failures: ["local: Asset Hub is down"],
    });
    expect(await readFile(join(dir, "local.json"))).toEqual(before);
  });

  test("a stack without an Asset Hub keeps no dotNS record", async () => {
    served = await at(100, "g1", { ...MODEL, chains: [MODEL.chains[0]] });
    await add("relay-only", link, undefined, options());
    expect(await files()).toEqual([
      "index.d.ts",
      "index.js",
      "relay-only.json",
      "stacks.json",
    ]);
  });

  test("remove drops the copies and the stack from the module", async () => {
    await add("local", link, undefined, options());
    await remove("local", { dir });
    expect(await files()).toEqual(["index.d.ts", "index.js", "stacks.json"]);
    const { stacks } = await import(`${join(dir, "index.js")}?removed`);
    expect(stacks).toEqual({});
  });

  test("a name must be usable as a file name and a key", async () => {
    await expect(add("../up", link, undefined, options())).rejects.toThrow(
      "is not a stack name",
    );
  });
});
