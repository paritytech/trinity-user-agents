// A stack's dotNS contracts, read from its Asset Hub as docs/rfcs/trinity-stack-descriptor.md
// describes: descriptors leave them out, so the chain and the descriptor never disagree.
import {
  type Descriptor,
  type Fetch,
  type Pointer,
  type Store,
  DEFAULT_TIMEOUT_MS,
  stackId,
} from "./index.js";

/** The protocol registry's keys, as dotNS's `DotnsConstants` names them. */
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

/** One of the protocol registry's keys. */
export type DotnsKey = (typeof DOTNS_KEYS)[number];

/** A stack's dotNS contracts, as its Asset Hub's gateway and protocol registry report them. */
export type Dotns = {
  /** The contract the `DotnsGateway` pallet dispatches to. */
  dispatcher: string;
  /** The protocol registry, which names every other dotNS contract. */
  protocolRegistry: string;
  /** Registry keys to addresses. Keys the registry leaves unset are absent. */
  addresses: Partial<Record<DotnsKey, string>>;
  /** The descriptor `generation` they were read under; a restart of the chains redeploys dotNS. */
  generation: string;
  /** When they were read, in Unix seconds. */
  resolvedAt: number;
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

/** How long a stored resolution counts as fresh when `maxAgeMs` is not given. */
export const DEFAULT_DOTNS_MAX_AGE_MS = 60 * 60 * 1000;

// twox128("DotnsGateway") ++ twox128("DispatcherAddress")
const DISPATCHER_ADDRESS_KEY =
  "0x57ca9018d648b40a8286a471e82c3808f31ce5ec278db3dff6304333f0019302";
const PROTOCOL_REGISTRY = "0x7656419f"; // protocolRegistry()
const TARGET = "0xcc1f2afa"; // TARGET()
const REGISTRY_GET = "0x8eaa6ac0"; // get(bytes32)
const UNSET = `0x${"0".repeat(40)}`;

type Contracts = Pick<Dotns, "dispatcher" | "protocolRegistry" | "addresses">;

/** What the store keeps: the contracts, or null for a stack without a dotNS gateway. */
type Kept = {
  generation: string;
  resolvedAt: number;
  contracts: Contracts | null;
};

type RpcReply = {
  id?: unknown;
  result?: unknown;
  error?: { code?: number; message?: string };
};

/**
 * The stack's dotNS contracts, or undefined when its Asset Hub has no gateway dispatcher set.
 *
 * With a store, an answer read under the descriptor's current `generation` and younger than
 * `maxAgeMs` comes back without asking the chain, and a refresh that fails falls back to it. It is
 * kept under the pointer's `stackId` with `#dotns` appended, so `read` can share the store.
 * Throws when the chain cannot be reached and nothing usable is stored, and, without a
 * `transport`, when the stack names no Asset Hub RPC, or no `eth-rpc` once a dispatcher is set.
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
  const milliseconds = (options.now ?? new Date()).getTime();
  if (Number.isNaN(milliseconds))
    throw new RangeError("now is an invalid Date");
  const now = Math.floor(milliseconds / 1000);
  const id = `${stackId(pointer)}#dotns`;
  const kept = decodeKept(await options.store?.get(id));
  const usable =
    kept?.generation === descriptor.generation && kept.resolvedAt <= now
      ? kept
      : undefined;
  const maxAgeMs = options.maxAgeMs ?? DEFAULT_DOTNS_MAX_AGE_MS;
  if (usable && (now - usable.resolvedAt) * 1000 < maxAgeMs)
    return toDotns(usable);
  try {
    const transport =
      options.transport ?? defaultTransport(descriptor, options);
    const fresh = {
      generation: descriptor.generation,
      resolvedAt: now,
      contracts: await lookUp(transport),
    };
    await options.store?.set(
      id,
      new TextEncoder().encode(JSON.stringify(fresh)),
    );
    return toDotns(fresh);
  } catch (error) {
    if (usable) return toDotns(usable);
    throw error;
  }
}

function toDotns({
  generation,
  resolvedAt,
  contracts,
}: Kept): Dotns | undefined {
  return contracts ? { ...contracts, generation, resolvedAt } : undefined;
}

/** What a store holds under a stack's dotNS id, or undefined when it holds nothing usable. */
function decodeKept(bytes: Uint8Array | undefined): Kept | undefined {
  if (!bytes) return undefined;
  let kept: Kept;
  try {
    kept = JSON.parse(new TextDecoder().decode(bytes));
  } catch {
    return undefined;
  }
  const contracts = kept?.contracts;
  const validContracts =
    contracts === null ||
    (isAddress(contracts?.dispatcher) &&
      isAddress(contracts.protocolRegistry) &&
      typeof contracts.addresses === "object" &&
      contracts.addresses !== null &&
      Object.entries(contracts.addresses).every(
        ([key, value]) =>
          DOTNS_KEYS.includes(key as DotnsKey) && isAddress(value),
      ));
  const valid =
    typeof kept?.generation === "string" &&
    Number.isSafeInteger(kept.resolvedAt) &&
    validContracts;
  return valid ? kept : undefined;
}

function isAddress(value: unknown): value is string {
  return typeof value === "string" && /^0x[0-9a-f]{40}$/.test(value);
}

async function lookUp(transport: DotnsTransport): Promise<Contracts | null> {
  const stored = await transport.storage(DISPATCHER_ADDRESS_KEY);
  if (stored === null) return null;
  const dispatcher = stored.toLowerCase();
  if (!isAddress(dispatcher))
    throw new Error(
      `DotnsGateway.DispatcherAddress holds ${stored}, not an address`,
    );
  const protocolRegistry = await address(
    transport,
    dispatcher,
    PROTOCOL_REGISTRY,
  ).catch(async (error: unknown) => {
    // Paseo Next v2's gateway is a RootGatewayDispatcher: it has no protocolRegistry() and
    // forwards to the PoP controller in TARGET().
    if (!(error instanceof DotnsRevert)) throw error;
    const controller = await address(transport, dispatcher, TARGET);
    return address(transport, controller, PROTOCOL_REGISTRY);
  });
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
        ethCall(options.fetch ?? fetch, url, to, data, timeoutMs),
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
    const message = reply.error.message ?? "";
    // Geth's code for a revert, which eth-rpc reports as "execution reverted".
    if (reply.error.code === 3 || /revert/i.test(message))
      throw new DotnsRevert(`${to} reverted for ${data}: ${message}`);
    throw new Error(`eth_call to ${to} on ${url} failed: ${message}`);
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
