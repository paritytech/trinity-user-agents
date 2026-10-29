import { describe, expect, test } from "bun:test";
import { type Fetch, memoryStore } from "./index.js";
import { type Bundled, createStackReader } from "./reader.js";
import {
  cases,
  file,
  pointer,
  pointerOf,
  publishedAt as at,
} from "./test-fixtures.js";
import { wasm } from "./wasm.js";

/** One step of a refresh vector: the one action it names, and what the reader must answer. */
type Step = {
  get?: number;
  refresh?: number;
  due?: number;
  restart?: number;
  now: number;
  fetched?: {
    document: string | null;
    etag: string | null;
    notModified: boolean;
  };
  build: string | null;
  expect: unknown;
};

/** Runs a refresh vector's steps against the wasm build's `StackReader`. */
async function run(vector: {
  name: string;
  pointer: { url: string; key: string | null };
  refresh: "never" | "once" | { maxAge: number };
  build: string | null;
  steps: Step[];
}) {
  const pointer = pointerOf(vector.pointer);
  const start = async (build: string | null) => {
    const reader =
      typeof vector.refresh === "string"
        ? new wasm.StackReader(vector.refresh)
        : new wasm.StackReader("maxAge", vector.refresh.maxAge);
    reader.load(pointer, await file(build), stored);
    return reader;
  };
  let stored: Uint8Array | undefined;
  let reader = await start(vector.build);
  const fetchOf = (request?: { ifNoneMatch?: string }) =>
    request ? { ifNoneMatch: request.ifNoneMatch ?? null } : null;
  for (const [index, step] of vector.steps.entries()) {
    let outcome: unknown;
    if (step.get !== undefined) {
      const read = reader.get(pointer, step.get);
      outcome = {
        serves: read.current?.descriptor.sequence ?? null,
        fetch: fetchOf(read.fetch),
      };
    } else if (step.refresh !== undefined) {
      outcome = { fetch: fetchOf(reader.refresh(pointer, step.refresh)) };
    } else if (step.due !== undefined) {
      outcome = reader.due(step.due).length > 0;
    } else if (step.fetched) {
      const { document, etag, notModified } = step.fetched;
      try {
        const refreshed = reader.fetched(
          pointer,
          await file(document),
          etag ?? undefined,
          notModified,
          step.now,
        );
        const { accepted } = refreshed;
        stored = refreshed.stored ?? stored;
        outcome = {
          status: accepted.status,
          serves: accepted.descriptor.sequence,
          stores: refreshed.stored !== undefined,
          newGeneration: accepted.newGeneration,
        };
      } catch (error) {
        outcome = { error: (error as { code: string }).code };
      }
    } else {
      reader = await start(step.build);
      continue;
    }
    expect(outcome, `${vector.name} step ${index}`).toEqual(step.expect);
  }
}

// The same files the Rust crate runs, so the wasm build cannot drift from the native one.
describe("refresh conformance vectors", () => {
  for (const vector of cases.refresh) test(vector.name, () => run(vector));
});

const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/** A server for `URL_` that answers each request with `answer`, recording the requests. */
function server(answer: (request: Request) => Response | Promise<Response>) {
  const requests: Request[] = [];
  const fetch: Fetch = async (url, init) => {
    const request = new Request(url, init);
    requests.push(request);
    return answer(request);
  };
  return { fetch, requests };
}

const down = () =>
  server(() => {
    throw new TypeError("connection refused");
  });

const bundle = async (document: Uint8Array): Promise<Bundled> => ({
  pointer,
  document: text(document),
});

describe("createStackReader", () => {
  test("once: callers share the first fetch and run on the build copy when the server is down", async () => {
    const { fetch, requests } = down();
    const build = await bundle(await at(100));
    const reader = createStackReader({
      bundled: [build],
      refresh: "once",
      fetch,
    });
    const [first, second] = await Promise.all([
      reader.get(build),
      reader.get(build),
    ]);
    expect(first.descriptor.sequence).toBe(100);
    expect(second.descriptor.sequence).toBe(100);
    await reader.get(build);
    expect(requests.length).toBe(1);
  });

  test("max age: answers at once, refreshes behind, and sends the ETag it was served", async () => {
    let clock = new Date(1_000_000);
    const newer = await at(200);
    const { fetch, requests } = server((request) =>
      request.headers.get("if-none-match") === "e1"
        ? new Response(null, { status: 304 })
        : new Response(newer, { headers: { etag: "e1" } }),
    );
    const reader = createStackReader({
      bundled: { local: await bundle(await at(100)) },
      refresh: { maxAgeMs: 60_000 },
      fetch,
      now: () => clock,
    });
    const changed = new Promise((resolve) => reader.onChange(resolve));
    expect((await reader.get(pointer)).descriptor.sequence).toBe(100);
    expect(await changed).toMatchObject({ newGeneration: false });
    expect((await reader.get(pointer)).descriptor.sequence).toBe(200);
    clock = new Date(1_060_000);
    await reader.refresh(pointer);
    expect(
      requests.map((request) => request.headers.get("if-none-match")),
    ).toEqual([null, "e1"]);
    expect((await reader.get(pointer)).descriptor.sequence).toBe(200);
    reader.close();
  });

  test("a store starts the next run from the newer copy, never an older one", async () => {
    const store = memoryStore();
    const newer = await at(200);
    const first = createStackReader({
      bundled: [await bundle(await at(100))],
      store,
      fetch: server(() => new Response(newer)).fetch,
    });
    expect((await first.get(pointer)).descriptor.sequence).toBe(200);
    const { fetch, requests } = down();
    const restarted = createStackReader({
      bundled: [await bundle(await at(100))],
      refresh: "never",
      store,
      fetch,
    });
    expect((await restarted.get(pointer)).descriptor.sequence).toBe(200);
    const rebuilt = createStackReader({
      bundled: [await bundle(await at(300))],
      refresh: "never",
      store,
      fetch,
    });
    expect((await rebuilt.get(pointer)).descriptor.sequence).toBe(300);
    expect(requests.length).toBe(0);
  });

  test("a stack passed to get runs from its own copy, and a store that failed is asked again", async () => {
    const { fetch, requests } = down();
    let failing = true;
    const reader = createStackReader({
      refresh: "never",
      fetch,
      store: {
        get: async () => {
          if (!failing) return undefined;
          failing = false;
          throw new Error("store is closing");
        },
        set: () => {},
      },
    });
    const build = await bundle(await at(100));
    await expect(reader.get(build)).rejects.toThrow("store is closing");
    expect((await reader.get(build)).descriptor.sequence).toBe(100);
    expect(requests.length).toBe(0);
  });

  test("a stack it holds no copy of is fetched, and fails when it cannot be", async () => {
    const reader = createStackReader({ refresh: "never", fetch: down().fetch });
    await expect(reader.get(pointer)).rejects.toMatchObject({
      code: "unreachable",
    });
    const document = await at(100);
    const served = createStackReader({
      refresh: "never",
      fetch: server(() => new Response(document)).fetch,
    });
    expect((await served.get(pointer)).descriptor.sequence).toBe(100);
  });

  test("require names every chain and service the stack lacks", async () => {
    const reader = createStackReader({
      bundled: [await bundle(await at(100))],
      refresh: "never",
    });
    expect(
      (await reader.get(pointer, { require: { chains: ["Relay"] } })).signed,
    ).toBe(true);
    await expect(
      reader.get(pointer, {
        require: { chains: ["Relay", "Bulletin"], services: ["eth-rpc"] },
      }),
    ).rejects.toThrow("local has no chain Bulletin, service eth-rpc");
  });

  test("a custom fetch's own bug surfaces, and the stack can be fetched again", async () => {
    let broken = true;
    const document = await at(100);
    const reader = createStackReader({
      refresh: "never",
      fetch: async () => {
        if (broken) throw new RangeError("bug in the caller's fetch");
        return new Response(document);
      },
    });
    await expect(reader.get(pointer)).rejects.toThrow(
      "bug in the caller's fetch",
    );
    broken = false;
    expect((await reader.get(pointer)).descriptor.sequence).toBe(100);
  });

  test("a max age under a second, or not finite, is refused rather than fetching on every call", () => {
    for (const maxAgeMs of [500, Infinity, Number.NaN])
      expect(() => createStackReader({ refresh: { maxAgeMs } })).toThrow(
        "maxAgeSeconds of at least 1",
      );
  });

  test("a listener that throws neither fails the fetch nor stops the others", async () => {
    const newer = await at(200);
    const reader = createStackReader({
      bundled: [await bundle(await at(100))],
      refresh: "once",
      fetch: server(() => new Response(newer)).fetch,
    });
    const seen: number[] = [];
    reader.onChange(() => {
      throw new Error("a listener's own bug");
    });
    reader.onChange(({ descriptor }) => seen.push(descriptor.sequence));
    const errors: unknown[] = [];
    const original = console.error;
    console.error = (error: unknown) => errors.push(error);
    const reported = globalThis.reportError;
    globalThis.reportError = (error: unknown) => errors.push(error);
    try {
      expect((await reader.get(pointer)).descriptor.sequence).toBe(200);
    } finally {
      console.error = original;
      globalThis.reportError = reported;
    }
    expect(seen).toEqual([200]);
    expect(errors).toEqual([new Error("a listener's own bug")]);
  });
});
