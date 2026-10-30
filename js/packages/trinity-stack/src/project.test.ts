import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Fetch, encodeLink } from "./index.js";
import {
  type ProjectOptions,
  add,
  generate,
  list,
  remove,
  update,
} from "./project.js";
import { createStackReader } from "./reader.js";
import { MODEL, URL_, pointer, publishedAt as at } from "./test-fixtures.js";

const link = encodeLink(pointer);

let dir: string;
let served: Uint8Array<ArrayBuffer> | undefined;
const serve: Fetch = async () =>
  served ? new Response(served.slice()) : new Response(null, { status: 503 });
const options = (): ProjectOptions => ({
  dir,
  fetch: serve,
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
      ".gitignore",
      ".npmignore",
      "index.d.ts",
      "index.js",
      "local.json",
      "stacks.json",
    ]);
    expect(await readFile(join(dir, ".gitignore"), "utf8")).toBe(
      "index.js\nindex.d.ts\n",
    );
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

  test("update in a project with no stacks writes nothing, so a scheduled job opens no pull request", async () => {
    const empty = join(dir, "empty");
    expect(await update([], { ...options(), dir: empty })).toEqual({
      changed: [],
      failures: [],
      warnings: [],
    });
    await expect(readdir(empty)).rejects.toThrow("ENOENT");
  });

  test("update writes the signed copy when the stack moved, and nothing otherwise", async () => {
    await add("local", link, undefined, options());
    expect(await update([], options())).toEqual({
      changed: [],
      failures: [],
      warnings: [],
    });
    served = await at(200, "g2");
    expect((await update([], options())).changed).toEqual(["local.json"]);
    const [stack] = await list({ dir });
    expect(stack.descriptor?.generation).toBe("g2");
  });

  test("generate rebuilds the git-ignored module from the committed copies, with no fetch", async () => {
    await add("local", link, undefined, options());
    await rm(join(dir, "index.js"));
    await rm(join(dir, "index.d.ts"));
    const offline = {
      dir,
      fetch: () => Promise.reject(new Error("generate must not fetch")),
    };
    expect((await generate(offline)).changed.sort()).toEqual([
      "index.d.ts",
      "index.js",
    ]);
    const { stacks } = await import(`${join(dir, "index.js")}?generated`);
    expect(Object.keys(stacks)).toEqual(["local"]);
    // A fresh clone before `prepare`: update rebuilds the module but reports no committed change.
    await rm(join(dir, "index.js"));
    expect((await update([], options())).changed).toEqual([]);
  });

  test("the ignore files are written once, so a project's own edits to them stay", async () => {
    await add("local", link, undefined, options());
    await writeFile(join(dir, ".gitignore"), "index.js\nindex.d.ts\n*.local\n");
    await rm(join(dir, ".npmignore"));
    expect((await generate({ dir })).changed).toEqual([]);
    served = await at(200);
    expect((await update([], options())).changed).toEqual(["local.json"]);
    expect(await readFile(join(dir, ".gitignore"), "utf8")).toContain(
      "*.local",
    );
    expect(await files()).not.toContain(".npmignore");
  });

  test("generate fails without stacks.json, and when a stack has no copy", async () => {
    await expect(generate({ dir: join(dir, "missing") })).rejects.toThrow(
      "no stacks.json",
    );
    await add("local", link, undefined, options());
    await rm(join(dir, "local.json"));
    await expect(generate({ dir })).rejects.toThrow("no copy of local");
  });

  test("update rejects a lower sequence, an unreachable stack and an unsigned one, naming the stack", async () => {
    await add("local", link, undefined, options());
    const before = await readFile(join(dir, "local.json"));
    served = await at(50);
    expect(await update([], options())).toEqual({
      changed: [],
      failures: [
        "local: https://stack.example/.well-known/trinity-stack.json serves a lower sequence than the committed copy",
      ],
      warnings: [],
    });
    served = undefined;
    expect((await update(["local"], options())).failures).toEqual([
      "local: https://stack.example/.well-known/trinity-stack.json is unreachable",
    ]);
    served = new TextEncoder().encode("not json");
    expect((await update(["local"], options())).failures).toEqual([
      "local: https://stack.example/.well-known/trinity-stack.json serves a descriptor that breaks the rule not-json",
    ]);
    expect(await readFile(join(dir, "local.json"))).toEqual(before);
    await expect(add("open", URL_, undefined, options())).rejects.toThrow(
      "must be signed",
    );
    await expect(update(["missing"], options())).rejects.toThrow(
      "no stack named missing",
    );
  });

  test("a copy past validUntil is still written, and named in warnings by add and update", async () => {
    served = await at(100, "g1", { ...MODEL, validUntil: 1 });
    const warnings = ["local: the copy is past validUntil"];
    expect((await add("local", link, undefined, options())).warnings).toEqual(
      warnings,
    );
    expect((await update([], options())).warnings).toEqual(warnings);
  });

  test("remove drops the copy and the stack from the module", async () => {
    await add("local", link, undefined, options());
    await remove("local", { dir });
    expect(await files()).toEqual([
      ".gitignore",
      ".npmignore",
      "index.d.ts",
      "index.js",
      "stacks.json",
    ]);
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
