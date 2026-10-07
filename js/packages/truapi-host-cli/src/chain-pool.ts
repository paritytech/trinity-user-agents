// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Pooled JSON-RPC connections, keyed by genesis hash.
//
// The core opens a chain connection per need and does not pool (measured:
// three People-chain sockets during pairing, plus one per boot). The web host
// hides that behind dotli's chain broker. This is the terminal equivalent,
// deliberately leaner.
//
//   - Request ids are rewritten per lease, so responses route exactly.
//   - Subscription tokens are learned from string-typed results (substrate's
//     JSON-RPC v2 subscribe calls all return the token as a string result),
//     and notifications route by `params.subscription`.
//   - Delivery is synchronous in socket arrival order, so sharing a socket
//     cannot introduce message reordering. Reordering is the class of bug
//     behind the chain_head operation-ordering hazard.
//   - Each socket carries a bounded number of leases (default 2), because
//     substrate nodes cap `chainHead_v1_follow` subscriptions per connection.
//     Pooling bounds sockets per endpoint at ceil(leases / cap) instead of N.

import type { JsonRpcConnection } from "@parity/truapi-host";
import { toHex } from "./hex.js";

/** Protocol role of a chain within the host's environment. */
export type ChainRole = "Relay" | "AssetHub" | "People" | "Bulletin";

export interface ChainEndpoint {
  /** WebSocket JSON-RPC endpoint. */
  rpc: string;
  /** Optional label used in logs and confirm prompts. */
  name?: string;
  /**
   * Protocol role this chain answers for. Endpoints with a role are
   * advertised through the core's `supportedChains` set. Endpoints without
   * one stay connectable but unadvertised.
   */
  role?: ChainRole;
}

/** Chains this host serves, keyed by `0x`-prefixed genesis hash. */
export type ChainEndpoints = Record<string, ChainEndpoint>;

/** The subset of WebSocket the pool needs. Injectable for tests. */
export interface SocketLike {
  addEventListener(type: string, listener: (event: unknown) => void): void;
  send(data: string): void;
  close(): void;
}

export interface ChainPoolOptions {
  endpoints: ChainEndpoints;
  /**
   * Leases sharing one socket. Substrate's `chainHead_v1_follow` is capped per
   * connection, so this stays conservative by default.
   */
  maxLeasesPerSocket?: number;
  /** Socket factory, injectable for tests. Defaults to node's `WebSocket`. */
  createSocket?: (url: string) => SocketLike;
  log?: (line: string) => void;
}

interface Lease {
  deliver(json: string): void;
  end(): void;
}

interface PooledSocket {
  socket: SocketLike;
  open: boolean;
  closed: boolean;
  outbox: string[];
  leases: Set<Lease>;
  /** rewritten request id -> owner and the id to restore on the response. */
  pending: Map<string, { lease: Lease; originalId: unknown }>;
  /** subscription token -> owning lease. */
  tokens: Map<string, Lease>;
}

export interface ChainPool {
  connect(genesisHash: Uint8Array | string): Promise<JsonRpcConnection>;
  /** Open sockets right now, per endpoint URL (observability and tests). */
  socketCounts(): Record<string, number>;
  closeAll(): void;
}

export function createChainPool(options: ChainPoolOptions): ChainPool {
  const {
    endpoints,
    maxLeasesPerSocket = 2,
    createSocket = (url) => new WebSocket(url) as unknown as SocketLike,
    log,
  } = options;
  const socketsByGenesis = new Map<string, PooledSocket[]>();
  let nextRequestId = 0;

  function openSocket(genesisHash: string, url: string): PooledSocket {
    const entry: PooledSocket = {
      socket: createSocket(url),
      open: false,
      closed: false,
      outbox: [],
      leases: new Set(),
      pending: new Map(),
      tokens: new Map(),
    };
    entry.socket.addEventListener("open", () => {
      entry.open = true;
      for (const request of entry.outbox.splice(0)) {
        entry.socket.send(request);
      }
    });
    entry.socket.addEventListener("message", (event) => {
      if (entry.closed) {
        return;
      }
      route(entry, String((event as { data: unknown }).data));
    });
    entry.socket.addEventListener("error", () => {
      log?.(`chain[${url}] socket error`);
    });
    entry.socket.addEventListener("close", () => {
      teardown(genesisHash, entry);
    });
    return entry;
  }

  function teardown(genesisHash: string, entry: PooledSocket): void {
    if (entry.closed) {
      return;
    }
    entry.closed = true;
    try {
      entry.socket.close();
    } catch {
      // Already gone.
    }
    for (const lease of [...entry.leases]) {
      lease.end();
    }
    entry.leases.clear();
    entry.pending.clear();
    entry.tokens.clear();
    const pool = socketsByGenesis.get(genesisHash);
    if (pool !== undefined) {
      const remaining = pool.filter((candidate) => candidate !== entry);
      if (remaining.length === 0) {
        socketsByGenesis.delete(genesisHash);
      } else {
        socketsByGenesis.set(genesisHash, remaining);
      }
    }
  }

  function route(entry: PooledSocket, json: string): void {
    let message: Record<string, unknown>;
    try {
      message = JSON.parse(json) as Record<string, unknown>;
    } catch {
      log?.(`chain: dropping unparseable message (${json.slice(0, 60)}…)`);
      return;
    }

    // A response: route by the rewritten id, restore the original.
    if (typeof message.method !== "string") {
      const owner =
        typeof message.id === "string"
          ? entry.pending.get(message.id)
          : undefined;
      if (owner === undefined) {
        // The owning lease closed, or the server sent an id we never issued.
        return;
      }
      entry.pending.delete(message.id as string);
      if (typeof message.result === "string") {
        // Every substrate subscribe call returns its token as a string result.
        // False positives (e.g. a hex string from `chainSpec_v1_genesisHash`)
        // are harmless: the entry simply never receives a notification.
        entry.tokens.set(message.result, owner.lease);
      }
      owner.lease.deliver(JSON.stringify({ ...message, id: owner.originalId }));
      return;
    }

    // A notification: route by subscription token.
    const params = message.params as { subscription?: unknown } | undefined;
    const token = params?.subscription;
    if (typeof token === "string" || typeof token === "number") {
      const owner = entry.tokens.get(String(token));
      if (owner === undefined) {
        // Token of a closed lease (its server-side subscription outlives the
        // lease until the socket closes), or one we failed to learn. Dropping
        // is safe for the former. The latter cannot happen for substrate's
        // string-result subscribe calls.
        return;
      }
      owner.deliver(json);
      return;
    }

    log?.(`chain: dropping unroutable message (${json.slice(0, 80)}…)`);
  }

  function sendFrom(entry: PooledSocket, lease: Lease, request: string): void {
    if (entry.closed) {
      return;
    }
    let rewritten = request;
    try {
      const message = JSON.parse(request) as Record<string, unknown>;
      if (message.id !== undefined && message.id !== null) {
        // "hcp" is the host-cli pool namespace. The prefix guarantees a
        // rewritten id can never collide with an id some lease chose itself.
        const poolId = `hcp:${String(nextRequestId++)}`;
        entry.pending.set(poolId, { lease, originalId: message.id });
        rewritten = JSON.stringify({ ...message, id: poolId });
      }
    } catch {
      // Not JSON we can rewrite. Forward as-is (its response, if any, will be
      // unroutable and dropped).
    }
    if (entry.open) {
      entry.socket.send(rewritten);
    } else {
      entry.outbox.push(rewritten);
    }
  }

  function dropLease(
    genesisHash: string,
    entry: PooledSocket,
    lease: Lease,
  ): void {
    entry.leases.delete(lease);
    for (const [id, owner] of [...entry.pending]) {
      if (owner.lease === lease) {
        entry.pending.delete(id);
      }
    }
    for (const [token, owner] of [...entry.tokens]) {
      if (owner === lease) {
        entry.tokens.delete(token);
      }
    }
    if (entry.leases.size === 0) {
      teardown(genesisHash, entry);
    }
  }

  return {
    async connect(genesisHash) {
      const genesis =
        typeof genesisHash === "string" ? genesisHash : toHex(genesisHash);
      const endpoint = endpoints[genesis];
      if (endpoint === undefined) {
        // Throwing tells the core no provider is available for this chain.
        throw new Error(`no RPC endpoint configured for ${genesis}`);
      }

      const pool = socketsByGenesis.get(genesis) ?? [];
      let entry = pool.find(
        (candidate) =>
          !candidate.closed && candidate.leases.size < maxLeasesPerSocket,
      );
      if (entry === undefined) {
        entry = openSocket(genesis, endpoint.rpc);
        socketsByGenesis.set(genesis, [...pool, entry]);
        log?.(
          `chain[${endpoint.name ?? genesis.slice(0, 12)}] opening socket #${String(
            (socketsByGenesis.get(genesis) ?? []).length,
          )}`,
        );
      }

      const inbox: string[] = [];
      let wake: (() => void) | null = null;
      let ended = false;
      const lease: Lease = {
        deliver(json) {
          if (ended) {
            return;
          }
          inbox.push(json);
          wake?.();
          wake = null;
        },
        end() {
          ended = true;
          wake?.();
          wake = null;
        },
      };
      entry.leases.add(lease);
      const owner = entry;

      const close = (): void => {
        if (ended) {
          return;
        }
        lease.end();
        dropLease(genesis, owner, lease);
      };

      return {
        send(request: string): void {
          if (!ended) {
            sendFrom(owner, lease, request);
          }
        },
        async *responses(): AsyncIterable<string> {
          try {
            while (!ended) {
              while (inbox.length > 0) {
                const next = inbox.shift();
                if (next !== undefined) {
                  yield next;
                }
              }
              if (ended) {
                break;
              }
              await new Promise<void>((resolve) => {
                wake = resolve;
              });
            }
          } finally {
            close();
          }
        },
        close,
      };
    },

    socketCounts() {
      const counts: Record<string, number> = {};
      for (const [genesis, pool] of socketsByGenesis) {
        const url = endpoints[genesis]?.rpc ?? genesis;
        counts[url] = (counts[url] ?? 0) + pool.filter((s) => !s.closed).length;
      }
      return counts;
    },

    closeAll() {
      for (const [genesis, pool] of [...socketsByGenesis]) {
        for (const entry of [...pool]) {
          teardown(genesis, entry);
        }
      }
      socketsByGenesis.clear();
    },
  };
}
