import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Fetch, encodeLink } from "./index.js";
import {
  type ProjectOptions,
  add,
  check,
  list,
  remove,
  update,
} from "./project.js";
import { createStackReader } from "./reader.js";
import {
  AT,
  MODEL,
  URL_,
  cases,
  pointer,
  publishedAt as at,
  transportOf,
} from "./test-fixtures.js";

const controller = cases.dotns[0];
const MOVED = `0x${"cc".repeat(20)}`;
const link = encodeLink(pointer);

let dir: string;
let served: Uint8Array<ArrayBuffer> | undefined;
const serve: Fetch = async () =>
  served ? new Response(served.slice()) : new Response(null, { status: 503 });
const options = (): ProjectOptions => ({
  dir,
  fetch: serve,
  transport: () => transportOf(controller),
});
const files = async () => (await readdir(dir)).sort();

beforeEach(async () => {
  dir = await mkdtemp(join(tmpdir(), "trinity-stack-"));
  served = await at(100);
});
afterEach(() => rm(dir, { recursive: true, force: true }));

describe("trinity-stack add and update", () => {
  test("add writes the signed copy and a module a reader runs from", async () => {
    const { changed } = await add("local", link, undefined, options());
    expect(changed.sort()).toEqual([
      "index.d.ts",
      "index.js",
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
    });
    expect((await reader.get(stacks.local)).descriptor.sequence).toBe(100);
    expect(await readFile(join(dir, "index.d.ts"), "utf8")).toContain(
      'readonly "local": Bundled;',
    );
  });

  test("update writes only the signed copy, and nothing when the stack did not move", async () => {
    await add("local", link, undefined, options());
    expect(await update([], options())).toEqual({ changed: [], failures: [] });
    served = await at(200, "g2");
    expect((await update([], options())).changed.sort()).toEqual([
      "index.js",
      "local.json",
    ]);
    const [stack] = await list({ dir });
    expect(stack.descriptor?.generation).toBe("g2");
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

  test("check reads a stack's committed copy by name, else a link or URL, else a file", async () => {
    served = await at(100, "g1", {
      ...MODEL,
      dotns: controller.descriptor.dotns,
    });
    await add("local", link, undefined, options());
    served = await at(200, "g1", {
      ...MODEL,
      dotns: { ...controller.descriptor.dotns, resolver: MOVED },
    });
    const drift = [
      {
        key: "resolver",
        stated: MOVED,
        onChain: controller.expect.found.resolver,
      },
    ];
    expect(await check("local", options())).toEqual({ at: AT, drift: [] });
    expect(await check(URL_, options())).toEqual({ at: AT, drift });
    expect(await check(link, options())).toEqual({ at: AT, drift });
    const path = join(dir, "moved.json");
    await writeFile(path, served);
    expect(await check(path, options())).toEqual({ at: AT, drift });
  });

  test("check rejects a descriptor without an Asset Hub", async () => {
    const path = join(dir, "relay-only.json");
    await writeFile(
      path,
      await at(100, "g1", { ...MODEL, chains: [MODEL.chains[0]] }),
    );
    await expect(check(path, options())).rejects.toThrow("no AssetHub chain");
  });

  test("remove drops the copy and the stack from the module", async () => {
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
    await expect(add("stacks", link, undefined, options())).rejects.toThrow(
      "stacks.json",
    );
    await add("constructor", link, undefined, options());
    expect((await list({ dir })).map(({ name }) => name)).toEqual([
      "constructor",
    ]);
  });

  test("a link under an https base is read as a link, not a descriptor URL", async () => {
    await add(
      "local",
      encodeLink(pointer, "https://app.example/open"),
      undefined,
      options(),
    );
    const [stack] = await list({ dir });
    expect(stack.descriptor?.sequence).toBe(100);
  });

  test("a stack whose link no longer reads fails alone", async () => {
    await add("local", link, undefined, options());
    await writeFile(
      join(dir, "stacks.json"),
      JSON.stringify({ broken: "not a link", local: link }),
    );
    served = await at(200);
    const { changed, failures } = await update([], options());
    expect(changed).toContain("local.json");
    expect(failures).toHaveLength(1);
    expect(failures[0]).toStartWith("broken: ");
  });
});
