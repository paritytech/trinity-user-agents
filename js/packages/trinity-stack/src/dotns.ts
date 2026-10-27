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

/** A stack's dotNS contracts, as its Asset Hub's gateway and protocol registry report them. */
export type Dotns = {
  /** The contract the `DotnsGateway` pallet dispatches to. */
  dispatcher: string;
  /** The protocol registry, which names every other dotNS contract. */
  protocolRegistry: string;
  /** Registry keys, such as `registrar`, to addresses. Keys the registry leaves unset are absent. */
  addresses: Record<string, string>;
  /** The descriptor `generation` they were read under; a restart of the chains redeploys dotNS. */
  generation: string;
  /** When they were read, in Unix seconds. */
  resolvedAt: number;
};

/** How `resolveDotns` reaches Asset Hub, for a reader with its own client, such as a light client. */
export type DotnsTransport = {
  /** The hex value at an Asset Hub storage key, or null when it is unset. */
  storage(key: string): Promise<string | null>;
  /** The hex result of an `eth_call` to `to` with `data`. Throws when the call reverts. */
  call(to: string, data: string): Promise<string>;
};

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

/** How long a stored resolution counts as fresh when `maxAgeMs` is not given. */
export const DEFAULT_DOTNS_MAX_AGE_MS = 60 * 60 * 1000;

// twox128("DotnsGateway") ++ twox128("DispatcherAddress")
const DISPATCHER_ADDRESS_KEY =
  "0x57ca9018d648b40a8286a471e82c3808f31ce5ec278db3dff6304333f0019302";
const PROTOCOL_REGISTRY = "0x7656419f"; // protocolRegistry()
const TARGET = "0xcc1f2afa"; // TARGET()
const REGISTRY_GET = "0x8eaa6ac0"; // get(bytes32)
const UNSET = `0x${"0".repeat(40)}`;

/**
 * The stack's dotNS contracts, or undefined when its Asset Hub has no gateway dispatcher set.
 *
 * With a store, a resolution read under the descriptor's current `generation` and younger than
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
  const id = `${stackId(pointer)}#dotns`;
  const now = Math.floor((options.now ?? new Date()).getTime() / 1000);
  const stored = await options.store?.get(id);
  const kept: Dotns | undefined =
    stored && JSON.parse(new TextDecoder().decode(stored));
  const usable = kept?.generation === descriptor.generation ? kept : undefined;
  const maxAgeMs = options.maxAgeMs ?? DEFAULT_DOTNS_MAX_AGE_MS;
  if (usable && (now - usable.resolvedAt) * 1000 < maxAgeMs) return usable;
  try {
    const transport =
      options.transport ?? defaultTransport(descriptor, options);
    const dotns = await lookUp(transport, descriptor.generation, now);
    if (dotns)
      await options.store?.set(
        id,
        new TextEncoder().encode(JSON.stringify(dotns)),
      );
    return dotns;
  } catch (error) {
    if (usable) return usable;
    throw error;
  }
}

async function lookUp(
  transport: DotnsTransport,
  generation: string,
  resolvedAt: number,
): Promise<Dotns | undefined> {
  const dispatcher = await transport.storage(DISPATCHER_ADDRESS_KEY);
  if (!dispatcher) return undefined;
  if (!/^0x[0-9a-f]{40}$/i.test(dispatcher))
    throw new Error(
      `DotnsGateway.DispatcherAddress holds ${dispatcher}, not an address`,
    );
  const protocolRegistry = await address(
    transport,
    dispatcher,
    PROTOCOL_REGISTRY,
  ).catch(
    // A gateway may point at a RootGatewayDispatcher, which forwards to the PoP controller in TARGET().
    async () =>
      address(
        transport,
        await address(transport, dispatcher, TARGET),
        PROTOCOL_REGISTRY,
      ),
  );
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
    dispatcher: dispatcher.toLowerCase(),
    protocolRegistry,
    addresses: Object.fromEntries(
      entries.filter(([, value]) => value !== UNSET),
    ),
    generation,
    resolvedAt,
  };
}

/** The address an `eth_call` returns as its one 32-byte word. */
async function address(
  transport: DotnsTransport,
  to: string,
  data: string,
): Promise<string> {
  const word = await transport.call(to, data);
  if (!/^0x[0-9a-f]{64}$/i.test(word))
    throw new Error(`${to} returned ${word} for ${data}`);
  return `0x${word.slice(-40).toLowerCase()}`;
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
  const ethRpc = descriptor.services.find(
    (service) => service.kind === "eth-rpc",
  )?.urls[0];
  if (rpc.length === 0)
    throw new Error(
      `${descriptor.id} names no Asset Hub RPC; pass a transport`,
    );
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  return {
    storage: async (key) => {
      let lastError: unknown;
      for (const url of rpc) {
        try {
          return await webSocketRequest(
            url,
            "state_getStorage",
            [key],
            timeoutMs,
          );
        } catch (error) {
          lastError = error;
        }
      }
      throw lastError;
    },
    call: async (to, data) => {
      if (!ethRpc)
        throw new Error(`${descriptor.id} names no eth-rpc; pass a transport`);
      const response = await (options.fetch ?? fetch)(ethRpc, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          method: "eth_call",
          params: [{ to, data }, "latest"],
        }),
        signal: AbortSignal.timeout(timeoutMs),
      });
      const { result, error } = await response.json();
      if (error) throw new Error(`eth_call to ${to} failed: ${error.message}`);
      return result;
    },
  };
}

function webSocketRequest(
  url: string,
  method: string,
  params: unknown[],
  timeoutMs: number,
): Promise<string | null> {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    const timer = setTimeout(() => {
      socket.close();
      reject(new Error(`${method} on ${url} timed out`));
    }, timeoutMs);
    socket.onopen = () =>
      socket.send(JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }));
    socket.onmessage = (event) => {
      clearTimeout(timer);
      socket.close();
      const { result, error } = JSON.parse(String(event.data));
      if (error)
        reject(new Error(`${method} on ${url} failed: ${error.message}`));
      else resolve(result ?? null);
    };
    socket.onerror = () => {
      clearTimeout(timer);
      reject(new Error(`${method} on ${url} could not connect`));
    };
  });
}
