import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { type DotnsRecord, type DotnsTransport, DotnsRevert } from "./dotns.js";
import {
  type Fetch,
  type Model,
  generateSecret,
  memoryStore,
  publicKey,
  publish,
} from "./index.js";
import { type Bundled, createStackReader } from "./reader.js";
import { wasm } from "./wasm.js";

const VECTORS = new URL(
  "../../../../rust/crates/trinity-stack/vectors/",
  import.meta.url,
);
const cases = JSON.parse(
  await readFile(new URL("cases.json", VECTORS), "utf8"),
);
const file = async (path: string | null) =>
  path === null
    ? undefined
    : new Uint8Array(await readFile(new URL(path, VECTORS)));
const pointerOf = (pointer: { url: string; key: string | null }) => ({
  url: pointer.url,
  key: pointer.key ?? undefined,
});

/** One step of a refresh vector: the one action it names, and what the reader must answer. */
type Step = {
  get?: number;
  refresh?: number;
  due?: number;
  restart?: number;
  dotns?: number;
  now: number;
  fetched?: {
    document: string | null;
    etag: string | null;
    notModified: boolean;
  };
  build: string | null;
  buildDotns: DotnsRecord | null;
  lookedUp?: DotnsRecord | null;
  expect: unknown;
};

/** Runs a refresh vector's steps against the wasm build's `StackReader`. */
async function run(vector: {
  name: string;
  pointer: { url: string; key: string | null };
  refresh: "never" | "once" | { maxAge: number };
  build: string | null;
  buildDotns: DotnsRecord | null;
  steps: Step[];
}) {
  const pointer = pointerOf(vector.pointer);
  const start = async (build: string | null, dotns: DotnsRecord | null) => {
    const reader =
      typeof vector.refresh === "string"
        ? new wasm.StackReader(vector.refresh)
        : new wasm.StackReader("maxAge", vector.refresh.maxAge);
    reader.load(pointer, await file(build), dotns ?? undefined, stored);
    return reader;
  };
  let stored: Uint8Array | undefined;
  let reader = await start(vector.build, vector.buildDotns);
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
        const { accepted, record } = reader.fetched(
          pointer,
          await file(document),
          etag ?? undefined,
          notModified,
          step.now,
        );
        stored = record ?? stored;
        outcome = {
          status: accepted.status,
          serves: accepted.descriptor.sequence,
          stores: record !== undefined,
          newGeneration: accepted.newGeneration,
        };
      } catch (error) {
        outcome = { error: (error as { code: string }).code };
      }
    } else if (step.restart !== undefined) {
      reader = await start(step.build, step.buildDotns);
      continue;
    } else if (step.dotns !== undefined) {
      const read = reader.dotns(pointer, step.dotns)!;
      outcome = {
        record: read.record?.generation ?? null,
        lookUp: read.lookUp ?? null,
      };
    } else {
      const looked = reader.lookedUp(
        pointer,
        step.lookedUp ?? undefined,
        step.now,
      );
      stored = looked.stored ?? stored;
      outcome = {
        record: looked.record?.generation ?? null,
        stores: looked.stored !== undefined,
      };
    }
    expect(outcome, `${vector.name} step ${index}`).toEqual(step.expect);
  }
}

// The same files the Rust crate runs, so the wasm build cannot drift from the native one.
describe("refresh conformance vectors", () => {
  for (const vector of cases.refresh) test(vector.name, () => run(vector));
});

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
  ],
  services: [],
};
const secret = generateSecret();
const pointer = { url: URL_, key: publicKey(secret) };
const text = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/** Signed copies of one stack: `at(n)` is published at second n, so a later one is newer. */
async function at(seconds: number, generation = "g1") {
  const { document } = await publish(MODEL, {
    generation,
    secret,
    now: new Date(seconds * 1000),
  });
  // A copy over a plain ArrayBuffer, which a Response body takes.
  return document.slice();
}

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

/** Asset Hub as a dotNS vector has it; `before` runs ahead of each lookup's block read. */
const chainOf = (
  vector: {
    at: { number: number; hash: string };
    storage: string | null;
    calls: Record<string, string | null>;
  },
  before: () => Promise<void> = async () => {},
): DotnsTransport => ({
  at: async () => {
    await before();
    return vector.at;
  },
  storage: async () => vector.storage,
  call: async (to, data) => {
    const answer = vector.calls[`${to}:${data}`];
    if (answer === null) throw new DotnsRevert("execution reverted");
    return answer;
  },
});

const bundle = async (
  document: Uint8Array,
  dotns: Bundled["dotns"] = null,
): Promise<Bundled> => ({ pointer, document: text(document), dotns });

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
        require: { chains: ["Relay", "AssetHub"], services: ["eth-rpc"] },
      }),
    ).rejects.toThrow("local has no chain AssetHub, service eth-rpc");
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

  test("dotNS: the build's record serves until the generation changes, then the chain is read", async () => {
    const vector = cases.dotns[0];
    let lookups = 0;
    const transport = () =>
      chainOf(vector, async () => {
        lookups++;
      });
    const restarted = await at(200, "g2");
    const reader = createStackReader({
      bundled: [
        await bundle(await at(100), {
          generation: "g1",
          at: vector.at,
          dotns: undefined,
        }),
      ],
      refresh: "never",
      fetch: server(() => new Response(restarted)).fetch,
      transport,
    });
    expect(await reader.dotns(pointer)).toBeUndefined();
    expect(lookups).toBe(0);
    await reader.refresh(pointer);
    expect((await reader.dotns(pointer))?.dispatcher).toBe(
      vector.expect.dotns.dispatcher,
    );
    expect(lookups).toBe(1);
    await reader.dotns(pointer);
    expect(lookups).toBe(1);
  });

  test("dotNS: a lookup the descriptor outran never answers for the new generation", async () => {
    const [g2Chain, g1Chain] = cases.dotns;
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    const restarted = await at(200, "g2");
    const reader = createStackReader({
      bundled: [await bundle(await at(100))],
      refresh: "never",
      fetch: server(() => new Response(restarted)).fetch,
      transport: (descriptor) =>
        descriptor.generation === "g1"
          ? chainOf(g1Chain, () => gate)
          : chainOf(g2Chain),
    });
    const first = reader.dotns(pointer);
    await reader.refresh(pointer);
    const second = reader.dotns(pointer);
    release();
    const g2 = g2Chain.expect.dotns.dispatcher;
    expect((await first)?.dispatcher).toBe(g2);
    expect((await second)?.dispatcher).toBe(g2);
  });

  test("a max age under a second, or not finite, is refused rather than fetching on every call", () => {
    for (const maxAgeMs of [500, Infinity, Number.NaN])
      expect(() => createStackReader({ refresh: { maxAgeMs } })).toThrow(
        "maxAgeSeconds of at least 1",
      );
  });

  test("dotNS: a failed lookup keeps the record of the same generation", async () => {
    const vector = cases.dotns[0];
    const record = {
      generation: "g1",
      at: vector.at,
      dotns: vector.expect.dotns,
    };
    const reader = createStackReader({
      bundled: [await bundle(await at(100), record)],
      refresh: "once",
      fetch: down().fetch,
      transport: () => ({
        at: () => Promise.reject(new Error("Asset Hub is down")),
        storage: () => Promise.reject(new Error("unused")),
        call: () => Promise.reject(new Error("unused")),
      }),
    });
    expect((await reader.dotns(pointer))?.dispatcher).toBe(
      vector.expect.dotns.dispatcher,
    );
  });
});
