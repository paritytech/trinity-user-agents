import { afterEach, describe, expect, test } from "bun:test";
import { checkDotns, readDotns } from "./dotns.js";
import { type Fetch, isTrinityStackError, parse } from "./index.js";
import { AT, type DotnsChain, cases, transportOf } from "./test-fixtures.js";

const DISPATCHER = `0x${"d1".repeat(20)}`;
const ETH_RPC = {
  kind: "eth-rpc",
  urls: ["http://127.0.0.1:8545"],
  chainId: 420420417,
};

/** A value as JS reads it back: an unset field is absent, where the vectors write null. */
const withoutNulls = (value: unknown) =>
  JSON.parse(JSON.stringify(value, (_, field) => field ?? undefined));

const parsed = (document: unknown) =>
  parse(new TextEncoder().encode(JSON.stringify(document))).descriptor;

/**
 * The first dotNS vector's descriptor with `services`, its Asset Hub on `assetHubRpc`, or without
 * an Asset Hub when none is given.
 */
function descriptor(assetHubRpc?: string, services: object[] = []) {
  const document = cases.dotns[0].descriptor;
  const chains = document.chains.flatMap((chain: { role: string }) =>
    chain.role !== "AssetHub"
      ? [chain]
      : assetHubRpc
        ? [{ ...chain, rpc: [assetHubRpc] }]
        : [],
  );
  return parsed({
    ...document,
    chains,
    services,
    assets: [],
    dotns: assetHubRpc ? document.dotns : undefined,
  });
}

/** A transport over `chain` that records the block of every read. */
function over(chain: DotnsChain) {
  const blocks: unknown[] = [];
  const transport = transportOf(chain, { onRead: (at) => blocks.push(at) });
  return { transport, blocks };
}

// The same files the Rust crate runs, so the wasm build cannot drift from the native one.
describe("dotNS conformance vectors", () => {
  for (const vector of cases.dotns) {
    test(vector.name, async () => {
      const stated = parsed(vector.descriptor);
      const transport = transportOf(vector);
      let outcome: unknown;
      try {
        const read = await readDotns(stated, { transport });
        const checked = await checkDotns(stated, { transport });
        outcome = { found: read.addresses, drift: checked.drift };
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
    const read = await readDotns(parsed(cases.dotns[0].descriptor), {
      transport,
    });
    expect(read.at).toEqual(AT);
    expect(blocks.length).toBeGreaterThan(0);
    expect(blocks.every((block) => block === AT)).toBe(true);
  });

  test("a call that fails keeps its own error rather than reading as a revert", async () => {
    const { transport } = over({ ...cases.dotns[0], calls: {} });
    const failing = {
      ...transport,
      call: () => Promise.reject(new Error("connection reset")),
    };
    await expect(
      readDotns(parsed(cases.dotns[0].descriptor), { transport: failing }),
    ).rejects.toThrow("connection reset");
  });

  test("a stack that names no Asset Hub RPC needs a transport", async () => {
    await expect(readDotns(descriptor())).rejects.toThrow(
      "names no Asset Hub RPC",
    );
  });
});

describe("over the stack's own endpoints", () => {
  let server: ReturnType<typeof Bun.serve> | undefined;
  let connections = 0;
  afterEach(() => {
    server?.stop(true);
    connections = 0;
  });

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
        open() {
          connections++;
        },
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

  test("reads storage and calls at the finalized block, over one connection", async () => {
    const storage: unknown[][] = [];
    const blocks: unknown[] = [];
    const rpc = assetHub(chainAt(DISPATCHER, storage));
    const read = await readDotns(descriptor(rpc, [ETH_RPC]), {
      fetch: ethRpc(blocks),
    });
    expect(read).toEqual({ at: AT, addresses: cases.dotns[0].expect.found });
    expect(storage).toEqual([[cases.dotns[0].storageKey, AT.hash]]);
    expect(new Set(blocks)).toEqual(new Set(["0x2c71c"]));
    expect(connections).toBe(1);
  });

  test("a reply that is not JSON fails the read instead of hanging", async () => {
    const rpc = assetHub(() => ["<html>502 Bad Gateway</html>"]);
    await expect(
      readDotns(descriptor(rpc, [ETH_RPC]), {
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
      readDotns(descriptor(rpc, [ETH_RPC]), { fetch: ethRpc() }),
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
    await readDotns(descriptor(rpc, [service]), { fetch: fetcher });
    expect(downCalls).toBe(1);
  });

  test("a dispatcher on a stack that names no eth-rpc needs a transport", async () => {
    const rpc = assetHub(chainAt(DISPATCHER));
    await expect(readDotns(descriptor(rpc))).rejects.toThrow(
      "names no eth-rpc",
    );
  });
});
