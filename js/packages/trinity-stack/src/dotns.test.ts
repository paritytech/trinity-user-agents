import { afterEach, describe, expect, test } from "bun:test";
import {
  type DotnsTransport,
  DOTNS_KEYS,
  DotnsRevert,
  resolveDotns,
} from "./dotns.js";
import { type Fetch, type Store, memoryStore, parse } from "./index.js";

const POINTER = { url: "https://stack.example/.well-known/trinity-stack.json" };
const DISPATCHER = `0x${"d1".repeat(20)}`;
const ROOT_DISPATCHER = `0x${"d0".repeat(20)}`;
const REGISTRY = `0x${"ee".repeat(20)}`;
const REGISTRAR = `0x${"aa".repeat(20)}`;
const RESOLVER = `0x${"bb".repeat(20)}`;
const ETH_RPC = {
  kind: "eth-rpc",
  urls: ["http://127.0.0.1:8545"],
  chainId: 420420417,
};

const word = (address: string) => `0x${address.slice(2).padStart(64, "0")}`;
const key = (name: string) =>
  `0x8eaa6ac0${Buffer.from(name).toString("hex").padEnd(64, "0")}`;
const at = (seconds: number) => new Date(seconds * 1000);

/** A descriptor of `generation`, with an Asset Hub on `assetHubRpc` when given. */
function descriptor(
  generation: string,
  assetHubRpc?: string,
  services: object[] = [],
) {
  const chains: object[] = [
    {
      role: "Relay",
      genesisHash: `0x${"86".repeat(32)}`,
      rpc: ["ws://127.0.0.1:9944"],
    },
  ];
  if (assetHubRpc)
    chains.push({
      role: "AssetHub",
      genesisHash: `0x${"bb".repeat(32)}`,
      paraId: 1000,
      rpc: [assetHubRpc],
    });
  const document = {
    $v: 1,
    sequence: 1,
    generation,
    id: "local",
    name: "Local",
    network: "local",
    tld: "local",
    services,
    chains,
  };
  return parse(new TextEncoder().encode(JSON.stringify(document))).descriptor;
}

const resolve = (
  generation: string,
  options: Parameters<typeof resolveDotns>[2],
) => resolveDotns(POINTER, descriptor(generation), options);

const ANSWERS = new Map([
  [`${DISPATCHER}:0x7656419f`, word(REGISTRY)],
  [`${ROOT_DISPATCHER}:0xcc1f2afa`, word(DISPATCHER)],
  [`${REGISTRY}:${key("registrar")}`, word(REGISTRAR)],
  [`${REGISTRY}:${key("resolver")}`, word(RESOLVER)],
]);

/** The eth_call answers of a registry holding `registrar` and `resolver`. */
function answer(to: string, data: string): string {
  const found = ANSWERS.get(`${to}:${data}`);
  if (found) return found;
  if (to === REGISTRY) return word(`0x${"0".repeat(40)}`);
  throw new DotnsRevert("execution reverted");
}

/** A chain whose gateway dispatches to `dispatcher`, counting the requests made to it. */
function chain(dispatcher: string | null) {
  let count = 0;
  const transport: DotnsTransport = {
    storage: async () => {
      count++;
      return dispatcher;
    },
    call: async (to, data) => {
      count++;
      return answer(to, data);
    },
  };
  return { transport, requests: () => count };
}

const DOWN: DotnsTransport = {
  storage: () => Promise.reject(new Error("unreachable")),
  call: () => Promise.reject(new Error("unreachable")),
};

const resolved = (dispatcher = DISPATCHER) => ({
  dispatcher,
  protocolRegistry: REGISTRY,
  addresses: { registrar: REGISTRAR, resolver: RESOLVER },
});

describe("resolveDotns", () => {
  test("follows the gateway's dispatcher to the registry and leaves unset keys out", async () => {
    const { transport, requests } = chain(DISPATCHER);
    expect(await resolve("g1", { transport })).toEqual(resolved());
    // The storage read, protocolRegistry(), then one get per key.
    expect(requests()).toBe(2 + DOTNS_KEYS.length);
  });

  test("reaches the registry through a dispatcher that forwards to TARGET()", async () => {
    const { transport } = chain(ROOT_DISPATCHER);
    expect(await resolve("g1", { transport })).toEqual(
      resolved(ROOT_DISPATCHER),
    );
  });

  test("a dispatcher that answers neither call is named, and a failed call keeps its own error", async () => {
    const neither: DotnsTransport = {
      storage: async () => `0x${"0c".repeat(20)}`,
      call: async (to, data) => answer(to, data),
    };
    await expect(resolve("g1", { transport: neither })).rejects.toThrow(
      "answers neither protocolRegistry() nor TARGET()",
    );
    const failing: DotnsTransport = {
      storage: async () => ROOT_DISPATCHER,
      call: async (to, data) => {
        if (data === "0x7656419f") throw new Error("connection reset");
        return answer(to, data);
      },
    };
    await expect(resolve("g1", { transport: failing })).rejects.toThrow(
      "connection reset",
    );
  });

  test("an answer that is not an address, or a word with high bytes set, is refused", async () => {
    for (const transport of [
      { storage: async () => "0x1234", call: DOWN.call },
      {
        storage: async () => DISPATCHER,
        call: async () => `0x${"ff".repeat(32)}`,
      },
    ])
      await expect(resolve("g1", { transport })).rejects.toThrow(
        "not an address",
      );
  });

  test("a stored answer is reused until it ages out or the chains restart", async () => {
    const store = memoryStore();
    const { transport, requests } = chain(DISPATCHER);
    const options = (seconds: number) => ({
      store,
      transport,
      maxAgeMs: 60_000,
      now: at(seconds),
    });
    await resolve("g1", options(100));
    const once = requests();
    expect(await resolve("g1", options(159))).toEqual(resolved());
    expect(requests()).toBe(once);
    await resolve("g1", options(160));
    expect(requests()).toBe(2 * once);
    await resolve("g2", options(161));
    expect(requests()).toBe(3 * once);
  });

  test("no gateway is asked again, since a restarted stack sets one after its chains start", async () => {
    const store = memoryStore();
    expect(
      await resolve("g1", {
        store,
        transport: chain(null).transport,
        now: at(100),
      }),
    ).toBeUndefined();
    expect(
      await resolve("g1", {
        store,
        transport: chain(DISPATCHER).transport,
        now: at(101),
      }),
    ).toEqual(resolved());
  });

  test("no gateway stands in for an unreachable chain like any other answer", async () => {
    const store = memoryStore();
    const { transport } = chain(null);
    await resolve("g1", { store, transport, now: at(100) });
    expect(
      await resolve("g1", { store, transport: DOWN, now: at(101) }),
    ).toBeUndefined();
  });

  test("a failed refresh falls back to the stored answer of the same generation only", async () => {
    const store = memoryStore();
    await resolve("g1", {
      store,
      transport: chain(DISPATCHER).transport,
      now: at(100),
    });
    const later = { store, transport: DOWN, maxAgeMs: 0, now: at(200) };
    expect(await resolve("g1", later)).toEqual(resolved());
    await expect(resolve("g2", later)).rejects.toThrow("unreachable");
  });

  test("a corrupt store entry is ignored and replaced", async () => {
    const { transport } = chain(DISPATCHER);
    for (const corrupt of [
      "{not json",
      "",
      '{"generation":"g1","resolvedAt":1}',
      JSON.stringify({
        generation: "g1",
        resolvedAt: 1,
        dotns: {
          dispatcher: DISPATCHER,
          protocolRegistry: REGISTRY,
          addresses: null,
        },
      }),
    ]) {
      const store: Store = memoryStore();
      store.set(`${POINTER.url}#dotns`, new TextEncoder().encode(corrupt));
      expect(await resolve("g1", { store, transport, now: at(100) })).toEqual(
        resolved(),
      );
      expect(
        await resolve("g1", { store, transport: DOWN, now: at(101) }),
      ).toEqual(resolved());
    }
  });

  test("a store that cannot be written fails the call instead of passing for an unreachable chain", async () => {
    const store = memoryStore();
    await resolve("g1", {
      store,
      transport: chain(DISPATCHER).transport,
      now: at(100),
    });
    const full: Store = {
      get: store.get,
      set: () => {
        throw new Error("disk full");
      },
    };
    await expect(
      resolve("g1", {
        store: full,
        transport: chain(DISPATCHER).transport,
        maxAgeMs: 0,
        now: at(200),
      }),
    ).rejects.toThrow("disk full");
  });

  test("a stack that names no Asset Hub RPC needs a transport", async () => {
    await expect(resolve("g1", {})).rejects.toThrow("names no Asset Hub RPC");
  });
});

describe("resolveDotns over the stack's own endpoints", () => {
  let server: ReturnType<typeof Bun.serve> | undefined;
  afterEach(() => server?.stop(true));

  /** An Asset Hub RPC that answers each request with `messages(id)`. */
  function assetHub(messages: (id: number) => string[]) {
    server = Bun.serve({
      port: 0,
      fetch: (request, bun) =>
        bun.upgrade(request, { data: undefined })
          ? undefined
          : new Response(null, { status: 400 }),
      websocket: {
        message(socket, message) {
          for (const reply of messages(JSON.parse(String(message)).id))
            socket.send(reply);
        },
      },
    });
    return `ws://127.0.0.1:${server.port}`;
  }

  const ethRpc: Fetch = async (_url, init) => {
    const { params } = JSON.parse(String(init?.body));
    try {
      return Response.json({
        id: 1,
        result: answer(params[0].to, params[0].data),
      });
    } catch {
      return Response.json({
        id: 1,
        error: { code: 3, message: "execution reverted" },
      });
    }
  };

  test("reads storage over the RPC and calls through eth-rpc, skipping messages that are not the reply", async () => {
    const rpc = assetHub((id) => [
      JSON.stringify({ jsonrpc: "2.0", method: "chain_newHead", params: {} }),
      JSON.stringify({ jsonrpc: "2.0", id, result: ROOT_DISPATCHER }),
    ]);
    expect(
      await resolveDotns(POINTER, descriptor("g1", rpc, [ETH_RPC]), {
        fetch: ethRpc,
      }),
    ).toEqual(resolved(ROOT_DISPATCHER));
  });

  test("a reply that is not JSON fails the read instead of hanging", async () => {
    const rpc = assetHub(() => ["<html>502 Bad Gateway</html>"]);
    await expect(
      resolveDotns(POINTER, descriptor("g1", rpc, [ETH_RPC]), {
        fetch: ethRpc,
        timeoutMs: 5_000,
      }),
    ).rejects.toThrow("something other than JSON");
  });

  test("a reply without a result fails the read instead of reading as an unset gateway", async () => {
    const rpc = assetHub((id) => [JSON.stringify({ jsonrpc: "2.0", id })]);
    await expect(
      resolveDotns(POINTER, descriptor("g1", rpc, [ETH_RPC]), {
        fetch: ethRpc,
      }),
    ).rejects.toThrow("answered without a result");
  });

  test("an eth-rpc endpoint that is down is tried once, not on every call", async () => {
    const rpc = assetHub((id) => [
      JSON.stringify({ jsonrpc: "2.0", id, result: DISPATCHER }),
    ]);
    const down = "http://127.0.0.1:1";
    let downCalls = 0;
    const fetcher: Fetch = async (url, init) => {
      if (url !== down) return ethRpc(url, init);
      downCalls++;
      throw new TypeError("connection refused");
    };
    const service = { ...ETH_RPC, urls: [down, ...ETH_RPC.urls] };
    expect(
      await resolveDotns(POINTER, descriptor("g1", rpc, [service]), {
        fetch: fetcher,
      }),
    ).toEqual(resolved());
    expect(downCalls).toBe(1);
  });

  test("a dispatcher on a stack that names no eth-rpc needs a transport", async () => {
    const rpc = assetHub((id) => [
      JSON.stringify({ jsonrpc: "2.0", id, result: DISPATCHER }),
    ]);
    await expect(resolveDotns(POINTER, descriptor("g1", rpc))).rejects.toThrow(
      "names no eth-rpc",
    );
  });
});
