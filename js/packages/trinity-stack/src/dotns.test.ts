import { afterEach, describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import {
  type DotnsTransport,
  DotnsRevert,
  decodeDotnsRecord,
  encodeDotnsRecord,
  readDotns,
} from "./dotns.js";
import { type Fetch, isTrinityStackError, parse } from "./index.js";

const VECTORS = new URL(
  "../../../../rust/crates/trinity-stack/vectors/",
  import.meta.url,
);
const cases = JSON.parse(
  await readFile(new URL("cases.json", VECTORS), "utf8"),
);

const DISPATCHER = `0x${"d1".repeat(20)}`;
const REGISTRY = `0x${"ee".repeat(20)}`;
const AT = cases.dotns[0].at;
const ETH_RPC = {
  kind: "eth-rpc",
  urls: ["http://127.0.0.1:8545"],
  chainId: 420420417,
};

/** A value as JS reads it back: an unset field is absent, where the vectors write null. */
const withoutNulls = (value: unknown) =>
  JSON.parse(JSON.stringify(value, (_, field) => field ?? undefined));

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

/** The answers a chain gives, keyed `to:data`; null is a revert. */
type Chain = { storage: string | null; calls: Record<string, string | null> };

/** A transport over `chain` that records the block of every read. */
function over(chain: Chain, at = AT) {
  const blocks: unknown[] = [];
  const transport: DotnsTransport = {
    at: async () => at,
    storage: async (_key, block) => {
      blocks.push(block);
      return chain.storage;
    },
    call: async (to, data, block) => {
      blocks.push(block);
      const answer = chain.calls[`${to}:${data}`];
      if (answer === undefined)
        throw new Error(`unexpected call ${to}:${data}`);
      if (answer === null) throw new DotnsRevert("execution reverted");
      return answer;
    },
  };
  return { transport, blocks };
}

// The same files the Rust crate runs, so the wasm build cannot drift from the native one.
describe("dotNS conformance vectors", () => {
  for (const vector of cases.dotns) {
    test(vector.name, async () => {
      const { transport } = over(vector, vector.at);
      let outcome: unknown;
      try {
        outcome = await readDotns(descriptor(vector.generation), { transport });
      } catch (error) {
        if (!isTrinityStackError(error)) throw error;
        outcome = { error: error.code };
      }
      expect(withoutNulls(outcome)).toEqual(withoutNulls(vector.expect));
    });
  }
});

describe("readDotns", () => {
  test("makes every read at the one block its transport names", async () => {
    const { transport, blocks } = over(cases.dotns[0]);
    await readDotns(descriptor("g1"), { transport });
    // The storage read, protocolRegistry(), then one get per registry key.
    expect(blocks.length).toBe(2 + 16);
    expect(blocks.every((block) => block === AT)).toBe(true);
  });

  test("a call that fails keeps its own error rather than reading as a revert", async () => {
    const { transport } = over({ storage: DISPATCHER, calls: {} });
    const failing = {
      ...transport,
      call: () => Promise.reject(new Error("connection reset")),
    };
    await expect(
      readDotns(descriptor("g1"), { transport: failing }),
    ).rejects.toThrow("connection reset");
  });

  test("a stack that names no Asset Hub RPC needs a transport", async () => {
    await expect(readDotns(descriptor("g1"))).rejects.toThrow(
      "names no Asset Hub RPC",
    );
  });

  test("a record round-trips as committed, and a bad address is refused", () => {
    const record = {
      generation: "g1",
      at: AT,
      dotns: {
        dispatcher: DISPATCHER,
        protocolRegistry: REGISTRY,
        addresses: {},
      },
    };
    const bytes = encodeDotnsRecord(record);
    expect(new TextDecoder().decode(bytes).endsWith("}\n")).toBe(true);
    expect(decodeDotnsRecord(bytes)).toEqual(record);
    const bad = new TextEncoder().encode(
      JSON.stringify({
        ...record,
        dotns: { ...record.dotns, dispatcher: "0x12" },
      }),
    );
    expect(() => decodeDotnsRecord(bad)).toThrow("dispatcher");
    const none = { generation: "g1", at: AT, dotns: undefined };
    expect(decodeDotnsRecord(encodeDotnsRecord(none))).toEqual(none);
  });
});

describe("readDotns over the stack's own endpoints", () => {
  let server: ReturnType<typeof Bun.serve> | undefined;
  afterEach(() => server?.stop(true));

  /** An Asset Hub RPC that answers each request with `messages(request)`. */
  function assetHub(
    messages: (request: {
      id: number;
      method: string;
      params: unknown[];
    }) => string[],
  ) {
    server = Bun.serve({
      port: 0,
      fetch: (request, bun) =>
        bun.upgrade(request, { data: undefined })
          ? undefined
          : new Response(null, { status: 400 }),
      websocket: {
        message(socket, message) {
          for (const reply of messages(JSON.parse(String(message))))
            socket.send(reply);
        },
      },
    });
    return `ws://127.0.0.1:${server.port}`;
  }

  /** An Asset Hub at block 0x2c71c with the gateway at `dispatcher`. */
  const chainAt =
    (dispatcher: string, storageParams: unknown[][] = []) =>
    (request: { id: number; method: string; params: unknown[] }) => {
      const reply = (result: unknown) => [
        JSON.stringify({ jsonrpc: "2.0", method: "chain_newHead", params: {} }),
        JSON.stringify({ jsonrpc: "2.0", id: request.id, result }),
      ];
      if (request.method === "chain_getFinalizedHead") return reply(AT.hash);
      if (request.method === "chain_getHeader")
        return reply({ number: `0x${AT.number.toString(16)}` });
      storageParams.push(request.params);
      return reply(dispatcher);
    };

  /** An eth-rpc over the controller vector's chain, recording each call's block tag. */
  const ethRpc =
    (blocks: unknown[] = []): Fetch =>
    async (_url, init) => {
      const { params } = JSON.parse(String(init?.body));
      blocks.push(params[1]);
      const answer = cases.dotns[0].calls[`${params[0].to}:${params[0].data}`];
      return Response.json(
        answer === null || answer === undefined
          ? { id: 1, error: { code: 3, message: "execution reverted" } }
          : { id: 1, result: answer },
      );
    };

  test("reads storage and calls at the finalized block", async () => {
    const storage: unknown[][] = [];
    const blocks: unknown[] = [];
    const rpc = assetHub(chainAt(DISPATCHER, storage));
    const record = await readDotns(descriptor("g1", rpc, [ETH_RPC]), {
      fetch: ethRpc(blocks),
    });
    expect(record).toEqual({ ...cases.dotns[0].expect, generation: "g1" });
    expect(storage).toEqual([
      [
        "0x57ca9018d648b40a8286a471e82c3808f31ce5ec278db3dff6304333f0019302",
        AT.hash,
      ],
    ]);
    expect(new Set(blocks)).toEqual(new Set(["0x2c71c"]));
  });

  test("a reply that is not JSON fails the read instead of hanging", async () => {
    const rpc = assetHub(() => ["<html>502 Bad Gateway</html>"]);
    await expect(
      readDotns(descriptor("g1", rpc, [ETH_RPC]), {
        fetch: ethRpc(),
        timeoutMs: 5_000,
      }),
    ).rejects.toThrow("something other than JSON");
  });

  test("a reply without a result fails the read instead of reading as an unset gateway", async () => {
    const rpc = assetHub(({ id, method }) =>
      method === "state_getStorage"
        ? [JSON.stringify({ jsonrpc: "2.0", id })]
        : chainAt(DISPATCHER)({ id, method, params: [] }),
    );
    await expect(
      readDotns(descriptor("g1", rpc, [ETH_RPC]), { fetch: ethRpc() }),
    ).rejects.toThrow("answered without a result");
  });

  test("an eth-rpc endpoint that is down is tried once, not on every call", async () => {
    const rpc = assetHub(chainAt(DISPATCHER));
    const down = "http://127.0.0.1:1";
    let downCalls = 0;
    const answer = ethRpc();
    const fetcher: Fetch = async (url, init) => {
      if (url !== down) return answer(url, init);
      downCalls++;
      throw new TypeError("connection refused");
    };
    const service = { ...ETH_RPC, urls: [down, ...ETH_RPC.urls] };
    await readDotns(descriptor("g1", rpc, [service]), { fetch: fetcher });
    expect(downCalls).toBe(1);
  });

  test("a dispatcher on a stack that names no eth-rpc needs a transport", async () => {
    const rpc = assetHub(chainAt(DISPATCHER));
    await expect(readDotns(descriptor("g1", rpc))).rejects.toThrow(
      "names no eth-rpc",
    );
  });
});
