// A stack's dotNS contracts, read from its Asset Hub as docs/rfcs/trinity-stack-descriptor.md
// describes: descriptors leave them out, so the chain and the descriptor never disagree. The host
// core finds the dispatcher the same way, in `discover_pop_controller`
// (rust/crates/truapi/src/host_logic/dotns_gateway.rs).
import {
  type Descriptor,
  type Fetch,
  type Pointer,
  type Store,
  DEFAULT_TIMEOUT_MS,
  stackId,
} from "./index.js";
import { unixSeconds } from "./time.js";

/** The protocol registry's address keys, as dotNS's `DotnsConstants` names them. */
export const DOTNS_KEYS = [
  "registrar",
  "controller",
  "registry",
  "reverseResolver",
  "popRules",
  "storeFactory",
  "resolver",
  "contentResolver",
  "popController",
  "popResolver",
  "popLens",
  "nameEscrow",
  "multicall3",
  "create3Factory",
  "nameWhitelist",
  "costModel",
] as const;

/** One of the protocol registry's address keys. */
export type DotnsKey = (typeof DOTNS_KEYS)[number];

/** A stack's dotNS contracts, as its Asset Hub's gateway and protocol registry report them. */
export type Dotns = {
  /** The contract the `DotnsGateway` pallet dispatches to. */
  dispatcher: string;
  /** The protocol registry, which names every other dotNS contract. */
  protocolRegistry: string;
  /** Registry keys to addresses. Keys the registry leaves unset are absent. */
  addresses: Partial<Record<DotnsKey, string>>;
};

/** How `resolveDotns` reaches Asset Hub, for a reader with its own client, such as a light client. */
export type DotnsTransport = {
  /** The hex value at an Asset Hub storage key, or null when it is unset. */
  storage(key: string): Promise<string | null>;
  /** The hex result of an `eth_call` to `to` with `data`. Throws a `DotnsRevert` when the call reverts. */
  call(to: string, data: string): Promise<string>;
};

/** A contract call that reverted, as opposed to one that could not be made. */
export class DotnsRevert extends Error {
  override name = "DotnsRevert";
}

const DEFAULT_MAX_AGE_MS = 60 * 60 * 1000;
// twox128("DotnsGateway") ++ twox128("DispatcherAddress")
const DISPATCHER_ADDRESS_KEY =
  "0x57ca9018d648b40a8286a471e82c3808f31ce5ec278db3dff6304333f0019302";
const PROTOCOL_REGISTRY = "0x7656419f"; // protocolRegistry()
const TARGET = "0xcc1f2afa"; // TARGET()
const REGISTRY_GET = "0x8eaa6ac0"; // get(bytes32)
const UNSET = `0x${"0".repeat(40)}`;

/** What the store keeps; `dotns` is null for a stack without a dotNS gateway. */
type Kept = { generation: string; resolvedAt: number; dotns: Dotns | null };

type RpcReply = {
  id?: unknown;
  result?: unknown;
  error?: { code?: number; message?: string };
};

/**
 * The stack's dotNS contracts, or undefined when its Asset Hub has no gateway dispatcher set.
 * With a store, under the pointer's `stackId` plus `#dotns`, an answer from the descriptor's
 * `generation` younger than `maxAgeMs` (an hour by default) skips the chain, and a refresh that
 * fails falls back to it. Throws when the chain cannot be reached and nothing usable is stored.
 */
export async function resolveDotns(
  pointer: Pointer,
  descriptor: Descriptor,
  options: {
    store?: Store;
    maxAgeMs?: number;
    now?: Date;
    transport?: DotnsTransport;
    fetch?: Fetch;
    timeoutMs?: number;
  } = {},
): Promise<Dotns | undefined> {
  const now = unixSeconds(options.now ?? new Date());
  const id = `${stackId(pointer)}#dotns`;
  const kept = decodeKept(await options.store?.get(id));
  const usable =
    kept?.generation === descriptor.generation && kept.resolvedAt <= now
      ? kept
      : undefined;
  if (
    usable &&
    (now - usable.resolvedAt) * 1000 < (options.maxAgeMs ?? DEFAULT_MAX_AGE_MS)
  )
    return usable.dotns ?? undefined;
  try {
    const dotns = await lookUp(
      options.transport ?? defaultTransport(descriptor, options),
    );
    const fresh: Kept = {
      generation: descriptor.generation,
      resolvedAt: now,
      dotns,
    };
    await options.store?.set(
      id,
      new TextEncoder().encode(JSON.stringify(fresh)),
    );
    return dotns ?? undefined;
  } catch (error) {
    if (usable) return usable.dotns ?? undefined;
    throw error;
  }
}

/** What a store holds under a stack's dotNS id, or undefined when it holds nothing usable. */
function decodeKept(bytes: Uint8Array | undefined): Kept | undefined {
  try {
    const kept = JSON.parse(new TextDecoder().decode(bytes));
    const dotns = kept.dotns;
    const valid =
      typeof kept.generation === "string" &&
      Number.isSafeInteger(kept.resolvedAt) &&
      (dotns === null ||
        (isAddress(dotns?.dispatcher) &&
          isAddress(dotns.protocolRegistry) &&
          typeof dotns.addresses === "object"));
    return valid ? kept : undefined;
  } catch {
    return undefined;
  }
}

function isAddress(value: unknown): value is string {
  return typeof value === "string" && /^0x[0-9a-f]{40}$/.test(value);
}

async function lookUp(transport: DotnsTransport): Promise<Dotns | null> {
  const stored = await transport.storage(DISPATCHER_ADDRESS_KEY);
  if (stored === null) return null;
  const dispatcher = stored.toLowerCase();
  if (!isAddress(dispatcher))
    throw new Error(
      `DotnsGateway.DispatcherAddress holds ${stored}, not an address`,
    );
  const protocolRegistry = await registryOf(transport, dispatcher);
  const entries = await Promise.all(
    DOTNS_KEYS.map(
      async (key) =>
        [
          key,
          await address(
            transport,
            protocolRegistry,
            REGISTRY_GET + bytes32(key),
          ),
        ] as const,
    ),
  );
  return {
    dispatcher,
    protocolRegistry,
    addresses: Object.fromEntries(
      entries.filter(([, value]) => value !== UNSET),
    ),
  };
}

/**
 * The registry the dispatcher's PoP controller names. The dispatcher is the controller, or a
 * `RootGatewayDispatcher`, which has no `protocolRegistry()` and forwards to the one in `TARGET()`.
 */
async function registryOf(
  transport: DotnsTransport,
  dispatcher: string,
): Promise<string> {
  try {
    return await address(transport, dispatcher, PROTOCOL_REGISTRY);
  } catch (error) {
    if (!(error instanceof DotnsRevert)) throw error;
  }
  let controller: string;
  try {
    controller = await address(transport, dispatcher, TARGET);
  } catch (error) {
    if (error instanceof DotnsRevert)
      throw new Error(
        `${dispatcher} answers neither protocolRegistry() nor TARGET()`,
      );
    throw error;
  }
  return address(transport, controller, PROTOCOL_REGISTRY);
}

/** The address an `eth_call` returns as its one ABI word: twelve zero bytes, then the address. */
async function address(
  transport: DotnsTransport,
  to: string,
  data: string,
): Promise<string> {
  const word = await transport.call(to, data);
  const match = /^0x0{24}([0-9a-f]{40})$/i.exec(word);
  if (!match)
    throw new Error(`${to} returned ${word} for ${data}, not an address`);
  return `0x${match[1].toLowerCase()}`;
}

/** An ASCII key as Solidity's `bytes32("...")` holds it: its bytes, padded on the right. */
function bytes32(key: string): string {
  const hex = [...key]
    .map((char) => char.charCodeAt(0).toString(16).padStart(2, "0"))
    .join("");
  return hex.padEnd(64, "0");
}

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
  return {
    storage: (key) =>
      firstAnswer(rpc, async (url) => {
        const value = await webSocketRequest(
          url,
          "state_getStorage",
          [key],
          timeoutMs,
        );
        if (value !== null && typeof value !== "string")
          throw new Error(
            `state_getStorage on ${url} returned ${JSON.stringify(value)}`,
          );
        return value;
      }),
    call: (to, data) => {
      if (ethRpc.length === 0)
        throw new Error(`${descriptor.id} names no eth-rpc; pass a transport`);
      return firstAnswer(ethRpc, (url) =>
        ethCall(fetcher, url, to, data, timeoutMs),
      );
    },
  };
}

/** The first endpoint's answer, trying the next on failure. A revert is an answer. */
async function firstAnswer<T>(
  urls: string[],
  ask: (url: string) => Promise<T>,
): Promise<T> {
  let lastError: unknown;
  for (const url of urls) {
    try {
      return await ask(url);
    } catch (error) {
      if (error instanceof DotnsRevert) throw error;
      lastError = error;
    }
  }
  throw lastError;
}

async function ethCall(
  fetcher: Fetch,
  url: string,
  to: string,
  data: string,
  timeoutMs: number,
): Promise<string> {
  const response = await fetcher(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      jsonrpc: "2.0",
      id: 1,
      method: "eth_call",
      params: [{ to, data }, "latest"],
    }),
    redirect: "error",
    signal: AbortSignal.timeout(timeoutMs),
  });
  const reply = (await response.json()) as RpcReply;
  if (reply.error) {
    // Code 3 is the Ethereum JSON-RPC code for a reverted call.
    if (reply.error.code === 3)
      throw new DotnsRevert(`${to} reverted for ${data}`);
    throw new Error(
      `eth_call to ${to} on ${url} failed: ${reply.error.message}`,
    );
  }
  if (typeof reply.result !== "string")
    throw new Error(
      `eth_call to ${to} on ${url} returned ${JSON.stringify(reply.result)}`,
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
      else settle(() => resolve(result ?? null));
    };
    socket.onerror = () => fail("could not connect");
    socket.onclose = () => fail("closed before answering");
  });
}
