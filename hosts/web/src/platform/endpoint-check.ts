/**
 * Asking a stack's RPC nodes which chain they serve, before the host relies
 * on them.
 *
 * A descriptor says which genesis hash each role has; nothing says the node
 * at its RPC URL serves that chain. Each node is asked for its genesis hash,
 * and People also for its `NetworkSuffix`, which must be the stack's TLD:
 * every identity derives under it. A wrong answer means the node serves
 * another chain. A node that does not answer in time is only unchecked: it
 * fails on first use, as any unreachable node does.
 *
 * This catches a stale or mistaken descriptor, not a hostile node, which can
 * answer anything. The light client checks what it reads; an RPC node is
 * trusted.
 */

import {
  CHAIN_ROLES,
  type ChainRole,
  type NetworkConfig,
} from "./network-config.js";

/** `twox128("NetworkSuffix") ++ twox128("NetworkSuffix")`: the People chain's identity suffix. */
const NETWORK_SUFFIX_KEY =
  "0x63c2d4c355d2d188c234d0a0669dc90863c2d4c355d2d188c234d0a0669dc908";

/** How long one node gets to answer. */
export const ENDPOINT_CHECK_MS = 5_000;

/** What one node answered. */
export interface EndpointCheck {
  role: ChainRole;
  rpc: string;
  outcome: "ok" | "wrong" | "unchecked";
  /** What was wrong, or why it is unchecked. Empty when ok. */
  detail: string;
}

/** Sends one JSON-RPC call to `url` and resolves with its result. */
export type RpcAsk = (
  url: string,
  method: string,
  params: unknown[],
  timeoutMs: number,
) => Promise<unknown>;

/** {@link RpcAsk} over a browser WebSocket: one socket per call, closed after it. */
export const webSocketAsk: RpcAsk = (url, method, params, timeoutMs) =>
  new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    // Settled before closing: some runtimes fire `close` from inside `close()`.
    const finish = (settle: () => void) => {
      clearTimeout(timer);
      settle();
      socket.close();
    };
    const timer = setTimeout(
      () =>
        finish(() =>
          reject(new Error(`no answer within ${timeoutMs / 1000} s`)),
        ),
      timeoutMs,
    );
    socket.onopen = () =>
      socket.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
    socket.onmessage = (event) =>
      finish(() => {
        try {
          const answer = JSON.parse(String(event.data)) as {
            result?: unknown;
            error?: { message?: string };
          };
          if (answer.error !== undefined)
            reject(new Error(answer.error.message ?? "an RPC error"));
          else resolve(answer.result);
        } catch (error) {
          reject(error);
        }
      });
    socket.onerror = () =>
      finish(() => reject(new Error("the connection failed")));
    socket.onclose = () =>
      finish(() => reject(new Error("the connection closed")));
  });

/** The UTF-8 text of a SCALE `Vec<u8>`, or null when `hex` is not one. */
export function decodeSuffix(hex: unknown): string | null {
  if (typeof hex !== "string" || !/^0x([0-9a-f]{2})+$/i.test(hex)) return null;
  const bytes = Uint8Array.from(hex.slice(2).match(/../g)!, (pair) =>
    Number.parseInt(pair, 16),
  );
  const mode = bytes[0] & 0b11;
  const [length, start] =
    mode === 0
      ? [bytes[0] >> 2, 1]
      : mode === 1 && bytes.length > 1
        ? [(bytes[0] | (bytes[1] << 8)) >> 2, 2]
        : [-1, 0];
  if (length < 0 || start + length !== bytes.length) return null;
  return new TextDecoder().decode(bytes.subarray(start));
}

/** One call's result, or why there is none. */
type Answer = { value: unknown } | { error: string };

async function checkOne(
  config: NetworkConfig,
  role: ChainRole,
  ask: RpcAsk,
  timeoutMs: number,
): Promise<EndpointCheck> {
  const { rpc } = config.chains[role];
  const expected = config.genesis[role].toLowerCase();
  const result = (outcome: EndpointCheck["outcome"], detail = "") => ({
    role,
    rpc,
    outcome,
    detail,
  });
  const call = (method: string, params: unknown[]): Promise<Answer> =>
    ask(rpc, method, params, timeoutMs).then(
      (value) => ({ value }),
      (error: unknown) => ({
        error: error instanceof Error ? error.message : String(error),
      }),
    );
  // Both questions at once, so a node costs one wait at most.
  const [genesis, raw] = await Promise.all([
    call("chain_getBlockHash", [0]),
    role === "people"
      ? call("state_getStorage", [NETWORK_SUFFIX_KEY])
      : undefined,
  ]);
  if ("error" in genesis) return result("unchecked", genesis.error);
  if (
    typeof genesis.value !== "string" ||
    genesis.value.toLowerCase() !== expected
  )
    return result(
      "wrong",
      `it serves the chain with genesis ${String(genesis.value)}, not ${expected}`,
    );
  if (raw === undefined) return result("ok");
  if ("error" in raw)
    return result("unchecked", `its NetworkSuffix was not read: ${raw.error}`);
  const suffix = decodeSuffix(raw.value);
  if (suffix === null)
    return result("unchecked", "it holds no readable NetworkSuffix");
  return suffix === config.networkSuffix
    ? result("ok")
    : result(
        "wrong",
        `its NetworkSuffix is ${suffix}, but the stack's TLD is ${config.networkSuffix}; identities derived under one would not match the other`,
      );
}

/** Ask each of `roles`' RPC nodes, all at once, which chain it serves. */
export function checkEndpoints(
  config: NetworkConfig,
  roles: readonly ChainRole[] = CHAIN_ROLES,
  ask: RpcAsk = webSocketAsk,
  timeoutMs = ENDPOINT_CHECK_MS,
): Promise<EndpointCheck[]> {
  return Promise.all(
    roles.map((role) => checkOne(config, role, ask, timeoutMs)),
  );
}
