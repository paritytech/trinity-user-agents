import { afterEach, describe, expect, test } from "bun:test";
import {
  type DotnsTransport,
  DOTNS_KEYS,
  DotnsRevert,
  resolveDotns,
} from "./dotns.js";
import { type Descriptor, type Fetch, memoryStore, parse } from "./index.js";

const POINTER = { url: "https://stack.example/.well-known/trinity-stack.json" };
const STORE_ID = `${POINTER.url}#dotns`;
const DISPATCHER = `0x${"d1".repeat(20)}`;
const ROOT_DISPATCHER = `0x${"d0".repeat(20)}`;
const REGISTRY = `0x${"ee".repeat(20)}`;
const REGISTRAR = `0x${"aa".repeat(20)}`;
const RESOLVER = `0x${"bb".repeat(20)}`;

const word = (address: string) => `0x${address.slice(2).padStart(64, "0")}`;
const key = (name: string) =>
  `0x8eaa6ac0${Buffer.from(name).toString("hex").padEnd(64, "0")}`;
const at = (seconds: number) => new Date(seconds * 1000);

function descriptor(
  generation: string,
  extra: { chains?: object[]; services?: object[] } = {},
): Descriptor {
  const document = {
    $v: 1,
    sequence: 1,
    generation,
    id: "local",
    name: "Local",
    network: "local",
    tld: "local",
    services: extra.services ?? [],
    chains: [
      {
        role: "Relay",
        genesisHash: `0x${"86".repeat(32)}`,
        rpc: ["ws://127.0.0.1:9944"],
      },
      ...(extra.chains ?? []),
    ],
  };
  return parse(new TextEncoder().encode(JSON.stringify(document))).descriptor;
}

const ANSWERS = new Map([
  [`${DISPATCHER}:0x7656419f`, word(REGISTRY)],
  [`${ROOT_DISPATCHER}:0xcc1f2afa`, word(DISPATCHER)],
  [`${REGISTRY}:${key("registrar")}`, word(REGISTRAR)],
  [`${REGISTRY}:${key("resolver")}`, word(RESOLVER)],
]);

/** The eth_call answers of a registry holding `registrar` and `resolver`. */
function answer(to: string, data: string): string {
  if (to === REGISTRY && data.startsWith("0x8eaa6ac0"))
    return ANSWERS.get(`${to}:${data}`) ?? word(`0x${"0".repeat(40)}`);
  const found = ANSWERS.get(`${to}:${data}`);
  if (!found) throw new DotnsRevert("execution reverted");
  return found;
}

/** A chain whose gateway dispatches to `dispatcher`, counting the requests made to it. */
function chain(dispatcher: string | null) {
  const calls = { count: 0 };
  const transport: DotnsTransport = {
    storage: async () => {
      calls.count++;
      return dispatcher;
    },
    call: async (to, data) => {
      calls.count++;
      return answer(to, data);
    },
  };
  return { transport, calls };
}

const DOWN: DotnsTransport = {
  storage: () => Promise.reject(new Error("unreachable")),
  call: () => Promise.reject(new Error("unreachable")),
};

const resolved = (
  generation: string,
  resolvedAt: number,
  dispatcher = DISPATCHER,
) => ({
  dispatcher,
  protocolRegistry: REGISTRY,
  addresses: { registrar: REGISTRAR, resolver: RESOLVER },
  generation,
  resolvedAt,
});

describe("resolveDotns", () => {
  test("follows the gateway's dispatcher to the registry and leaves unset keys out", async () => {
    const { transport, calls } = chain(DISPATCHER);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        transport,
        now: at(100),
      }),
    ).toEqual(resolved("g1", 100));
    // The storage read, protocolRegistry(), then one get per key.
    expect(calls.count).toBe(2 + DOTNS_KEYS.length);
  });

  test("reaches the registry through a dispatcher that forwards to TARGET()", async () => {
    const { transport } = chain(ROOT_DISPATCHER);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        transport,
        now: at(100),
      }),
    ).toEqual(resolved("g1", 100, ROOT_DISPATCHER));
  });

  test("only a revert sends it to TARGET(), so a failed call keeps its own error", async () => {
    const transport: DotnsTransport = {
      storage: async () => DISPATCHER,
      call: async (to, data) => {
        if (data === "0x7656419f") throw new Error("connection reset");
        return answer(to, data);
      },
    };
    await expect(
      resolveDotns(POINTER, descriptor("g1"), { transport }),
    ).rejects.toThrow("connection reset");
  });

  test("an answer that is not an address, or a word with high bytes set, is refused", async () => {
    const notAnAddress: DotnsTransport = {
      storage: async () => "0x1234",
      call: DOWN.call,
    };
    await expect(
      resolveDotns(POINTER, descriptor("g1"), { transport: notAnAddress }),
    ).rejects.toThrow("not an address");
    const dirtyWord: DotnsTransport = {
      storage: async () => DISPATCHER,
      call: async () => `0x${"ff".repeat(32)}`,
    };
    await expect(
      resolveDotns(POINTER, descriptor("g1"), { transport: dirtyWord }),
    ).rejects.toThrow("not an address");
  });

  test("a stored answer is reused until it ages out or the chains restart", async () => {
    const store = memoryStore();
    const { transport, calls } = chain(DISPATCHER);
    const options = { store, transport, maxAgeMs: 60_000 };
    await resolveDotns(POINTER, descriptor("g1"), { ...options, now: at(100) });
    const afterFirst = calls.count;
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        ...options,
        now: at(159),
      }),
    ).toEqual(resolved("g1", 100));
    expect(calls.count).toBe(afterFirst);
    await resolveDotns(POINTER, descriptor("g1"), { ...options, now: at(160) });
    expect(calls.count).toBe(2 * afterFirst);
    expect(
      await resolveDotns(POINTER, descriptor("g2"), {
        ...options,
        now: at(161),
      }),
    ).toEqual(resolved("g2", 161));
  });

  test("a stack without a gateway is remembered as one", async () => {
    const store = memoryStore();
    const { transport } = chain(null);
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        store,
        transport,
        now: at(100),
      }),
    ).toBeUndefined();
    expect(
      await resolveDotns(POINTER, descriptor("g1"), {
        store,
        transport: DOWN,
        now: at(101),
      }),
    ).toBeUndefined();
  });

  test("a failed refresh falls back to the stored answer of the same generation only", async () => {
    const store = memoryStore();
    const { transport } = chain(DISPATCHER);
    await resolveDotns(POINTER, descriptor("g1"), {
      store,
      transport,
      now: at(100),
    });
    const later = { store, transport: DOWN, maxAgeMs: 0, now: at(200) };
    expect(await resolveDotns(POINTER, descriptor("g1"), later)).toEqual(
      resolved("g1", 100),
    );
    await expect(
      resolveDotns(POINTER, descriptor("g2"), later),
    ).rejects.toThrow("unreachable");
  });

  test("a corrupt store entry is ignored and replaced", async () => {
    const { transport } = chain(DISPATCHER);
    for (const corrupt of [
      "{not json",
      "",
      '{"generation":"g1","resolvedAt":1}',
    ]) {
      const store = memoryStore();
      store.set(STORE_ID, new TextEncoder().encode(corrupt));
      expect(
        await resolveDotns(POINTER, descriptor("g1"), {
          store,
          transport,
          now: at(100),
        }),
      ).toEqual(resolved("g1", 100));
      expect(
        await resolveDotns(POINTER, descriptor("g1"), {
          store,
          transport: DOWN,
          now: at(101),
        }),
      ).toEqual(resolved("g1", 100));
    }
  });

  test("a stack that names no Asset Hub RPC needs a transport", async () => {
    await expect(resolveDotns(POINTER, descriptor("g1"))).rejects.toThrow(
      "names no Asset Hub RPC",
    );
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

  const stack = (rpc: string, services: object[]) =>
    descriptor("g1", {
      chains: [
        {
          role: "AssetHub",
          genesisHash: `0x${"bb".repeat(32)}`,
          paraId: 1000,
          rpc: [rpc],
        },
      ],
      services,
    });
  const ETH_RPC = {
    kind: "eth-rpc",
    urls: ["http://127.0.0.1:8545"],
    chainId: 420420417,
  };

  test("reads storage over the RPC and calls through eth-rpc, skipping messages that are not the reply", async () => {
    const rpc = assetHub((id) => [
      JSON.stringify({ jsonrpc: "2.0", method: "chain_newHead", params: {} }),
      JSON.stringify({ jsonrpc: "2.0", id, result: ROOT_DISPATCHER }),
    ]);
    expect(
      await resolveDotns(POINTER, stack(rpc, [ETH_RPC]), {
        fetch: ethRpc,
        now: at(100),
      }),
    ).toEqual(resolved("g1", 100, ROOT_DISPATCHER));
  });

  test("a reply that is not JSON fails the read instead of hanging", async () => {
    const rpc = assetHub(() => ["<html>502 Bad Gateway</html>"]);
    await expect(
      resolveDotns(POINTER, stack(rpc, [ETH_RPC]), {
        fetch: ethRpc,
        timeoutMs: 5_000,
      }),
    ).rejects.toThrow("something other than JSON");
  });

  test("a dispatcher on a stack that names no eth-rpc needs a transport", async () => {
    const rpc = assetHub((id) => [
      JSON.stringify({ jsonrpc: "2.0", id, result: DISPATCHER }),
    ]);
    await expect(resolveDotns(POINTER, stack(rpc, []))).rejects.toThrow(
      "names no eth-rpc",
    );
  });
});
