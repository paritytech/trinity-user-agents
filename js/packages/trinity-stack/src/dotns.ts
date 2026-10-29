// A stack's dotNS contracts, read from its Asset Hub as docs/rfcs/trinity-stack-descriptor.md
// describes: descriptors leave them out, so the chain and the descriptor never disagree. The
// trinity-stack crate's `DotnsLookup` decides every step; this module only makes the reads.
import type { Block, Descriptor, DotnsRecord } from "../wasm/trinity_stack.js";
import { type Fetch, DEFAULT_TIMEOUT_MS } from "./index.js";
import { wasm } from "./wasm.js";

export type { Dotns, DotnsRecord } from "../wasm/trinity_stack.js";

/** How `readDotns` reaches Asset Hub, for a reader with its own client, such as a light client. */
export type DotnsTransport = {
  /** The block every read is made at, such as the latest finalized one. */
  at(): Promise<Block>;
  /** The hex value at an Asset Hub storage key at `at`, or null when it is unset. */
  storage(key: string, at: Block): Promise<string | null>;
  /** The hex result of a call to `to` with `data` at `at`. Throws a `DotnsRevert` when it reverts. */
  call(to: string, data: string, at: Block): Promise<string>;
};

/** A contract call that reverted, as opposed to one that could not be made. */
export class DotnsRevert extends Error {
  override name = "DotnsRevert";
}

/**
 * Reads the stack's dotNS contracts from its Asset Hub, every read at one block. The record's
 * `dotns` is undefined when the Asset Hub has no dotNS gateway. Throws when the chain cannot be
 * reached, or answers in a way no dotNS deployment does.
 */
export async function readDotns(
  descriptor: Descriptor,
  options: {
    transport?: DotnsTransport;
    fetch?: Fetch;
    timeoutMs?: number;
  } = {},
): Promise<DotnsRecord> {
  const transport = options.transport ?? defaultTransport(descriptor, options);
  const at = await transport.at();
  const lookup = new wasm.DotnsLookup(descriptor.generation, at);
  try {
    for (;;) {
      const step = lookup.next();
      if ("done" in step) return step.done;
      if ("storage" in step)
        lookup.storage(
          (await transport.storage(step.storage, at)) ?? undefined,
        );
      else
        lookup.answer(
          await Promise.all(
            step.calls.map(({ to, input }) =>
              transport.call(to, input, at).catch((error: unknown) => {
                if (error instanceof DotnsRevert) return null;
                throw error;
              }),
            ),
          ),
        );
    }
  } finally {
    lookup.free();
  }
}

/** A record as a project commits it, next to the stack's signed copy. */
export function encodeDotnsRecord(record: DotnsRecord): Uint8Array {
  return wasm.encodeDotnsRecord(record);
}

/** Reads a record `encodeDotnsRecord` wrote, checking every address. */
export function decodeDotnsRecord(bytes: Uint8Array): DotnsRecord {
  return wasm.decodeDotnsRecord(bytes);
}

type RpcReply = {
  id?: unknown;
  result?: unknown;
  error?: { code?: number; message?: string };
};

/** Asset Hub's RPC for the block and storage, and the stack's `eth-rpc` for calls. */
function defaultTransport(
  descriptor: Descriptor,
  options: { fetch?: Fetch; timeoutMs?: number },
): DotnsTransport {
  const rpc =
    descriptor.chains.find((chain) => chain.role === "AssetHub")?.rpc ?? [];
  const ethRpc =
    descriptor.services.find((service) => service.kind === "eth-rpc")?.urls ??
    [];
  if (rpc.length === 0)
    throw new Error(
      `${descriptor.id} names no Asset Hub RPC; pass a transport`,
    );
  const fetcher = options.fetch ?? fetch;
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  const askRpc = failover(rpc);
  const askEthRpc = failover(ethRpc);
  const request = (method: string, params: unknown[]) =>
    askRpc((url) => webSocketRequest(url, method, params, timeoutMs));
  return {
    at: async () => {
      const hash = await request("chain_getFinalizedHead", []);
      const header = (await request("chain_getHeader", [hash])) as {
        number?: unknown;
      } | null;
      if (typeof hash !== "string" || typeof header?.number !== "string")
        throw new Error(
          `Asset Hub's finalized head ${String(hash)} has no header`,
        );
      return { number: Number(header.number), hash };
    },
    storage: async (key, at) => {
      const value = await request("state_getStorage", [key, at.hash]);
      if (value !== null && typeof value !== "string")
        throw new Error(`state_getStorage returned ${JSON.stringify(value)}`);
      return value;
    },
    call: (to, data, at) => {
      if (ethRpc.length === 0)
        throw new Error(`${descriptor.id} names no eth-rpc; pass a transport`);
      const block = `0x${at.number.toString(16)}`;
      return askEthRpc((url) =>
        ethCall(fetcher, url, { to, data }, block, timeoutMs),
      );
    },
  };
}

/**
 * Asks `urls` in turn until one answers, starting from the one that answered last, so an
 * endpoint that is down costs its timeout once rather than on every call. A revert is an answer.
 */
function failover(urls: string[]) {
  let first = 0;
  return async <T>(ask: (url: string) => Promise<T>): Promise<T> => {
    let lastError: unknown;
    for (let offset = 0; offset < urls.length; offset++) {
      const index = (first + offset) % urls.length;
      try {
        const answer = await ask(urls[index]);
        first = index;
        return answer;
      } catch (error) {
        if (error instanceof DotnsRevert) {
          first = index;
          throw error;
        }
        lastError = error;
      }
    }
    throw lastError;
  };
}

async function ethCall(
  fetcher: Fetch,
  url: string,
  call: { to: string; data: string },
  block: string,
  timeoutMs: number,
): Promise<string> {
  const response = await fetcher(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [call, block],
    }),
    redirect: "error",
    signal: AbortSignal.timeout(timeoutMs),
  });
  const reply = (await response.json()) as RpcReply;
  if (reply.error) {
    // Code 3 is the Ethereum JSON-RPC code for a reverted call.
    if (reply.error.code === 3)
      throw new DotnsRevert(`${call.to} reverted for ${call.data}`);
    throw new Error(
      `eth_call to ${call.to} on ${url} failed: ${reply.error.message}`,
    );
  }
  if (typeof reply.result !== "string")
    throw new Error(
      `eth_call to ${call.to} on ${url} returned ${JSON.stringify(reply.result)}`,
    );
  return reply.result;
}

/** Sends one JSON-RPC request and settles on its reply, ignoring any other message. */
function webSocketRequest(
  url: string,
  method: string,
  params: unknown[],
  timeoutMs: number,
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    let settled = false;
    const settle = (finish: () => void) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      socket.close();
      finish();
    };
    const fail = (reason: string) =>
      settle(() => reject(new Error(`${method} on ${url} ${reason}`)));
    const timer = setTimeout(() => fail("timed out"), timeoutMs);
    socket.onopen = () =>
      socket.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
    socket.onmessage = (event) => {
      let reply: RpcReply | null;
      try {
        reply = JSON.parse(String(event.data));
      } catch {
        return fail("answered with something other than JSON");
      }
      if (reply?.id !== 1) return;
      const { result, error } = reply;
      if (error) fail(`failed: ${error.message}`);
      else if (result === undefined) fail("answered without a result");
      else settle(() => resolve(result));
    };
    socket.onerror = () => fail("could not connect");
    socket.onclose = () => fail("closed before answering");
  });
}
