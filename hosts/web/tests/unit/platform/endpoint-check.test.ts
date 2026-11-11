import { describe, expect, test } from "bun:test";
import {
  checkEndpoints,
  decodeSuffix,
  webSocketAsk,
  type RpcAsk,
} from "../../../src/platform/endpoint-check.js";
import { NETWORKS } from "../../../src/platform/network-config.js";

const PASEO = NETWORKS.paseo;
/** SCALE `Vec<u8>` of `paseo`. */
const PASEO_SUFFIX = "0x14706173656f";

/** Nodes that answer as `answers` says, by URL and method. */
function nodes(
  answers: (url: string, method: string) => unknown | Error,
): RpcAsk {
  return async (url, method) => {
    const answer = answers(url, method);
    if (answer instanceof Error) throw answer;
    return answer;
  };
}

/** Each node answers with its own chain's genesis, and People with `suffix`. */
const honest = (suffix = PASEO_SUFFIX) =>
  nodes((url, method) => {
    if (method === "state_getStorage") return suffix;
    const role = (
      Object.keys(PASEO.chains) as (keyof typeof PASEO.chains)[]
    ).find((key) => PASEO.chains[key].rpc === url)!;
    return PASEO.genesis[role];
  });

describe("checkEndpoints", () => {
  test("passes nodes that serve the descriptor's chains and TLD", async () => {
    const checks = await checkEndpoints(PASEO, undefined, honest());
    expect(checks.map((check) => check.outcome)).toEqual([
      "ok",
      "ok",
      "ok",
      "ok",
    ]);
  });

  // A node on another chain would have the core sign for, and read, the wrong chain.
  test("finds a node serving another chain", async () => {
    const other = `0x${"22".repeat(32)}`;
    const checks = await checkEndpoints(
      PASEO,
      ["assetHub"],
      nodes(() => other),
    );
    expect(checks).toEqual([
      {
        role: "assetHub",
        rpc: PASEO.chains.assetHub.rpc,
        outcome: "wrong",
        detail: `it serves the chain with genesis ${other}, not ${PASEO.genesis.assetHub}`,
      },
    ]);
  });

  // Every identity derives under the TLD, so People must agree with the descriptor.
  test("finds a People chain whose NetworkSuffix is not the stack's TLD", async () => {
    const [people] = await checkEndpoints(
      PASEO,
      ["people"],
      honest("0x1c746573746e6574"),
    );
    expect(people.outcome).toBe("wrong");
    expect(people.detail).toContain("its NetworkSuffix is testnet");
  });

  // An unreachable node is not a wrong one: it fails on first use, as any does.
  test("leaves a node that does not answer unchecked", async () => {
    const [relay] = await checkEndpoints(
      PASEO,
      ["relay"],
      nodes(() => new Error("no answer within 5 s")),
    );
    expect([relay.outcome, relay.detail]).toEqual([
      "unchecked",
      "no answer within 5 s",
    ]);
  });

  // Asked one after the other, a slow People node would hold the boot for two timeouts.
  test("asks People for its genesis and suffix at once", async () => {
    const asked: string[] = [];
    const answers = honest();
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    const checking = checkEndpoints(PASEO, ["people"], async (...call) => {
      asked.push(call[1]);
      await held;
      return answers(...call);
    });
    await Promise.resolve();
    expect(asked).toEqual(["chain_getBlockHash", "state_getStorage"]);
    release();
    expect((await checking)[0].outcome).toBe("ok");
  });
});

// Bun fires `close` from inside `close()`, so an answer settled after closing is lost.
describe("webSocketAsk", () => {
  test("returns the node's answer", async () => {
    const server = Bun.serve({
      port: 0,
      fetch: (request, bun) =>
        bun.upgrade(request) ? undefined : new Response(null, { status: 400 }),
      websocket: {
        message(socket, text) {
          const { id } = JSON.parse(String(text)) as { id: number };
          socket.send(JSON.stringify({ jsonrpc: "2.0", id, result: "0x01" }));
        },
      },
    });
    try {
      expect(
        await webSocketAsk(
          `ws://localhost:${server.port}`,
          "chain_getBlockHash",
          [0],
          5_000,
        ),
      ).toBe("0x01");
    } finally {
      await server.stop(true);
    }
  });
});

describe("decodeSuffix", () => {
  test("reads a SCALE Vec<u8>, and nothing else", () => {
    expect(decodeSuffix(PASEO_SUFFIX)).toBe("paseo");
    expect(decodeSuffix("0x0c646f74")).toBe("dot");
    expect(decodeSuffix(null)).toBeNull();
    expect(decodeSuffix("0x147061")).toBeNull();
  });
});
