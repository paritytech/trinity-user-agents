import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import {
  type Model,
  type SpecFile,
  accept,
  checkSpec,
  decodeLink,
  encodeLink,
  generateSecret,
  isTrinityStackError,
  memoryStore,
  MAX_DESCRIPTOR_BYTES,
  fetchDescriptor,
  open,
  parse,
  publicKey,
  publish,
  read,
  sameStack,
  stackId,
} from "./index.js";

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
const codeOf = (run: () => unknown) => {
  try {
    run();
    return "ok";
  } catch (error) {
    if (!isTrinityStackError(error)) throw error;
    return error.code;
  }
};
const pointerOf = (pointer: { url: string; key: string | null }) => ({
  url: pointer.url,
  key: pointer.key ?? undefined,
});

// The same files the Rust crate runs, so the wasm build cannot drift from the native one.
describe("conformance vectors", () => {
  for (const vector of cases.documents) {
    test(`document ${vector.name}`, async () => {
      const bytes = (await file(vector.file))!;
      expect(codeOf(() => open(bytes, pointerOf(vector.pointer)))).toBe(
        vector.expect,
      );
    });
  }

  for (const vector of cases.reader) {
    test(`reader ${vector.name}`, async () => {
      const stored = await file(vector.stored);
      const fetched = await file(vector.fetched);
      let outcome: unknown;
      try {
        const accepted = accept(
          pointerOf(vector.pointer),
          stored,
          fetched,
          new Date(vector.now * 1000),
        );
        const keeps =
          stored && Buffer.from(accepted.document).equals(Buffer.from(stored))
            ? "stored"
            : "fetched";
        outcome = {
          status: accepted.status,
          ...("refused" in accepted ? { refused: accepted.refused } : {}),
          keeps,
          newGeneration: accepted.newGeneration,
          stale: accepted.stale,
          expired: accepted.expired,
        };
      } catch (error) {
        if (!isTrinityStackError(error)) throw error;
        outcome = { error: error.code };
      }
      expect(outcome).toEqual(vector.expect);
    });
  }

  for (const vector of cases.links) {
    test(`link ${vector.name}`, () => {
      let outcome: unknown;
      try {
        outcome = decodeLink(vector.link);
      } catch (error) {
        if (!isTrinityStackError(error)) throw error;
        outcome = { error: error.code };
      }
      expect(outcome).toEqual(vector.expect);
    });
  }
});

const MODEL: Model = {
  id: "local",
  name: "Local",
  network: "local",
  tld: "local",
  chains: [
    {
      role: "Relay",
      genesisHash:
        "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
      rpc: ["ws://127.0.0.1:9944"],
    },
  ],
  services: [{ kind: "dub", urls: ["http://127.0.0.1:8080/"] }],
};
const URL_ = "https://stack.example/.well-known/trinity-stack.json";

describe("publish", () => {
  test("the key order a generator emits does not change the content", async () => {
    const first = await publish(MODEL, { generation: "g1" });
    const reordered = Object.fromEntries(
      Object.entries(MODEL).reverse(),
    ) as Model;
    const again = await publish(reordered, {
      generation: "g1",
      state: first.state,
    });
    expect(again.state).toEqual(first.state);
    // Same sequence must mean same bytes, or a reader holding the first sees a conflict.
    expect(again.document).toEqual(first.document);
  });

  test("a service kind only newer readers know still raises sequence", async () => {
    const first = await publish(MODEL, { generation: "g1" });
    const future = {
      ...MODEL,
      services: [
        ...MODEL.services,
        { kind: "future-kind", urls: ["https://a.example/"] },
      ],
    };
    const again = await publish(future, {
      generation: "g1",
      state: first.state,
    });
    expect(again.state.sequence).toBeGreaterThan(first.state.sequence);
  });

  test("an invalid Date is refused rather than turning off expiry", () => {
    expect(() =>
      accept({ url: URL_ }, undefined, new Uint8Array(), new Date("bad")),
    ).toThrow("invalid Date");
  });

  test("keeps sequence while nothing changes and raises it when something does", async () => {
    const now = new Date("2026-10-15T12:00:00Z");
    const first = await publish(MODEL, { generation: "g1", now });
    expect(first.state.sequence).toBe(1_792_065_600);

    const again = await publish(MODEL, {
      generation: "g1",
      state: first.state,
      now: new Date("2026-10-16T00:00:00Z"),
    });
    expect(again.state).toEqual(first.state);

    const renamed = await publish(
      { ...MODEL, name: "Renamed" },
      { generation: "g1", state: first.state, now },
    );
    expect(renamed.state.sequence).toBe(first.state.sequence + 1);
  });

  test("a clock behind the stored sequence still moves forward", async () => {
    const state = { sequence: 2_000_000_000, contentHash: "0x00" };
    const published = await publish(MODEL, {
      generation: "g1",
      state,
      now: new Date("2026-10-15T12:00:00Z"),
    });
    expect(published.state.sequence).toBe(2_000_000_001);
  });

  test("a signed publish opens under the matching key only", async () => {
    const secret = generateSecret();
    const { document } = await publish(MODEL, { generation: "g1", secret });
    expect(open(document, { url: URL_, key: publicKey(secret) }).signed).toBe(
      true,
    );
    expect(
      codeOf(() =>
        open(document, { url: URL_, key: publicKey(generateSecret()) }),
      ),
    ).toBe("bad-signature");
  });

  test("refuses a model that breaks a rule before anything is signed", async () => {
    const broken = {
      ...MODEL,
      services: [{ kind: "dub", urls: ["http://stack.example/"] }],
    };
    await expect(
      publish(broken, { generation: "g1", secret: generateSecret() }),
    ).rejects.toHaveProperty("code", "invalid-url");
  });

  test("a model never overrides the fields publish owns", async () => {
    const stale = { ...MODEL, $v: 2, sequence: 7, generation: "g0" } as Model;
    const { descriptor } = await publish(stale, {
      generation: "g1",
      now: new Date("2026-10-15T12:00:00Z"),
    });
    expect([descriptor.$v, descriptor.sequence, descriptor.generation]).toEqual(
      [1, 1_792_065_600, "g1"],
    );
  });
});

describe("read", () => {
  test("results are plain objects with bytes and maps as JS expects", async () => {
    const contracts = { registry: `0x${"11".repeat(20)}` };
    const { document } = await publish(
      { ...MODEL, contracts },
      { generation: "g1" },
    );
    const accepted = accept({ url: URL_ }, undefined, document);
    expect(accepted.document).toBeInstanceOf(Uint8Array);
    expect(Object.getPrototypeOf(accepted.descriptor.contracts)).toBe(
      Object.prototype,
    );
    expect(accepted.descriptor.contracts).toEqual(contracts);
    expect(accepted.descriptor.fork).toBeUndefined();
    expect("fork" in accepted.descriptor).toBe(false);
  });

  test("parse says whether a document is an envelope without checking any key", async () => {
    const plain = await publish(MODEL, { generation: "g1" });
    const signed = await publish(MODEL, {
      generation: "g1",
      secret: generateSecret(),
    });
    expect(parse(plain.document)).toEqual({
      descriptor: plain.descriptor,
      envelope: false,
    });
    expect(parse(signed.document).envelope).toBe(true);
  });

  test("a server that never answers is unreachable after the timeout", async () => {
    const secret = generateSecret();
    const pointer = { url: URL_, key: publicKey(secret) };
    const { document } = await publish(MODEL, { generation: "g1", secret });
    const store = memoryStore();
    await store.set(pointer.key, document);
    const hanging = (_: string, init?: RequestInit) =>
      new Promise<Response>((_, reject) =>
        init?.signal?.addEventListener("abort", () =>
          reject(init.signal!.reason),
        ),
      );
    const accepted = await read(pointer, {
      store,
      fetch: hanging,
      timeoutMs: 20,
    });
    expect(accepted.status).toBe("unreachable");
  });

  test("a body stream that cannot be iterated, as in Safari before 27, still reads", async () => {
    const bytes = new TextEncoder().encode("{}");
    const body = new Blob([bytes]).stream();
    Object.defineProperty(body, Symbol.asyncIterator, { value: undefined });
    const fetch = async () =>
      ({ ok: true, headers: new Headers(), body }) as unknown as Response;
    expect(await fetchDescriptor(URL_, { fetch })).toEqual(bytes);
  });

  test("a body past the size cap is unreachable", async () => {
    const huge = new Uint8Array(MAX_DESCRIPTOR_BYTES + 1);
    const streaming = async () => new Response(new Blob([huge]).stream());
    expect(await fetchDescriptor(URL_, { fetch: streaming })).toBeUndefined();
    const declared = async () =>
      new Response("{}", {
        headers: { "content-length": String(MAX_DESCRIPTOR_BYTES + 1) },
      });
    expect(await fetchDescriptor(URL_, { fetch: declared })).toBeUndefined();
  });

  test("a redirect is unreachable, whatever it points at", async () => {
    let redirect: RequestRedirect | undefined;
    const redirecting = async (
      _: string,
      init?: RequestInit,
    ): Promise<Response> => {
      redirect = init?.redirect;
      throw new TypeError("redirect mode is set to error");
    };
    expect(await fetchDescriptor(URL_, { fetch: redirecting })).toBeUndefined();
    expect(redirect).toBe("error");
  });

  test("an error that is not the network's surfaces", async () => {
    const broken = async (): Promise<Response> => {
      throw new Error("bug in a custom fetch");
    };
    await expect(read({ url: URL_ }, { fetch: broken })).rejects.toThrow(
      "bug in a custom fetch",
    );
  });

  test("a store turns a replayed older descriptor into a rollback", async () => {
    const secret = generateSecret();
    const pointer = { url: URL_, key: publicKey(secret) };
    const older = await publish(MODEL, {
      generation: "g1",
      secret,
      now: new Date("2026-10-15T12:00:00Z"),
    });
    const newer = await publish(
      { ...MODEL, name: "Newer" },
      { generation: "g1", secret, state: older.state },
    );
    const serving = (document: Uint8Array) => async () =>
      new Response(document.slice());
    const store = memoryStore();

    expect(
      (await read(pointer, { store, fetch: serving(newer.document) })).status,
    ).toBe("updated");
    const replayed = await read(pointer, {
      store,
      fetch: serving(older.document),
    });
    expect(replayed.status).toBe("rollback");
    expect(replayed.descriptor.name).toBe("Newer");
    expect(
      (
        await read(pointer, {
          store,
          fetch: async () => new Response(null, { status: 503 }),
        })
      ).status,
    ).toBe("unreachable");
  });
});

describe("packaging", () => {
  // A browser bundler must find nothing it cannot resolve, such as a `node:` import.
  test("bundles for the browser", async () => {
    const built = await Bun.build({
      entrypoints: [new URL("./index.ts", import.meta.url).pathname],
      target: "browser",
    });
    expect(built.success).toBe(true);
    // Bun keeps a `node:` import external; esbuild and webpack refuse it for a browser target.
    const bundle = await built.outputs[0]!.text();
    expect(bundle).not.toMatch(/(import\s*\(|from)\s*["']node:/);
  });

  test("a URL that is not one is the caller's bug, not an outage", async () => {
    await expect(fetchDescriptor("not a url")).rejects.toThrow();
  });
});

describe("links", () => {
  test("encode and decode round trip, and the key decides the stack", () => {
    const secret = generateSecret();
    const pointer = { url: URL_, key: publicKey(secret) };
    expect(decodeLink(encodeLink(pointer))).toEqual(pointer);
    expect(
      sameStack(pointer, {
        url: "https://moved.example/s.json",
        key: pointer.key,
      }),
    ).toBe(true);
    expect(sameStack(pointer, { url: URL_ })).toBe(false);
    expect(decodeLink(encodeLink({ url: URL_ }))).toStrictEqual({ url: URL_ });
  });

  test("a pointer breaking a rule keeps its code, and a key of the wrong type is never dropped", () => {
    expect(codeOf(() => stackId({ url: "http://stack.example/s.json" }))).toBe(
      "invalid-url",
    );
    const key = 1 as unknown as string;
    expect(() => stackId({ url: URL_, key })).toThrow(TypeError);
    expect(() =>
      checkSpec({ url: URL_ } as SpecFile, new Uint8Array()),
    ).toThrow(TypeError);
  });

  test("an unsigned stack is stored under its normalised URL", async () => {
    const { document } = await publish(MODEL, { generation: "g1" });
    const store = memoryStore();
    await read(
      { url: URL_ },
      { store, fetch: async () => new Response(document.slice()) },
    );
    expect(stackId({ url: `${URL_}#top` })).toBe(stackId({ url: URL_ }));
    expect(await store.get(stackId({ url: `${URL_}#top` }))).toEqual(document);
  });
});
