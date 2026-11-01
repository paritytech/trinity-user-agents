import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { readStatement } from "../../tools/statements/decode.js";

/**
 * A People chain connection that answers Statement Store calls as this
 * host's embedded light client does: smoldot-light 2.1.0,
 * `src/json_rpc_service/statement.rs` and `background.rs`, with smoldot
 * 4.0.0's `json_rpc/methods.rs` for the wire shapes.
 *
 * - `statement_subscribeStatement` answers the subscription id, then one
 *   empty page with `remaining: 0`. Later pages come only from
 *   {@link LightClientStatements.peersSend}, without `remaining`, once per
 *   statement per subscription.
 * - `statement_submit` checks expiry, size and proof in that order, then
 *   broadcasts to the peers {@link peers} describes: `{"status":"new"}` when
 *   at least one took it, else an RPC error 7001. It never answers `known`
 *   and never gives a peer's refusal.
 */
export class LightClientStatements {
  /** Gossip peers now: how many are connected, and how many take a broadcast. */
  peers = { connected: 3, accepting: 3 };
  /** Seconds since the Unix epoch, for the expiry check. */
  nowSeconds = Math.floor(Date.now() / 1000);
  /** Every statement that went out, `0x` hex. */
  readonly broadcast: string[] = [];
  private readonly connections = new Set<{
    push(text: string): void;
    subscriptions: Map<string, { topics: Set<string>; seen: Set<string> }>;
  }>();
  private nextSubscription = 1;

  connect(): JsonRpcConnection {
    const queue: string[] = [];
    let wake = () => {};
    let closed = false;
    const state = {
      push: (text: string) => {
        queue.push(text);
        wake();
      },
      subscriptions: new Map<
        string,
        { topics: Set<string>; seen: Set<string> }
      >(),
    };
    this.connections.add(state);
    return {
      send: (text) => this.answer(state, JSON.parse(text)),
      async *responses() {
        for (;;) {
          if (queue.length > 0) {
            yield queue.shift()!;
            continue;
          }
          if (closed) return;
          await new Promise<void>((resolve) => (wake = resolve));
        }
      },
      close: () => {
        closed = true;
        this.connections.delete(state);
        wake();
      },
    };
  }

  /** Peers send `statements` (`0x` hex): each open subscription whose filter matches gets them once. */
  peersSend(statements: string[]): void {
    for (const connection of this.connections)
      for (const [id, subscription] of connection.subscriptions) {
        const matching = statements.filter((hex) => {
          const { meta } = readStatement(hexToBytes(hex));
          if (!meta.topics.some((topic) => subscription.topics.has(topic)))
            return false;
          if (subscription.seen.has(meta.hash)) return false;
          subscription.seen.add(meta.hash);
          return true;
        });
        if (matching.length > 0)
          connection.push(
            JSON.stringify({
              jsonrpc: "2.0",
              method: "statement_statement",
              params: {
                subscription: id,
                result: {
                  event: "newStatements",
                  data: { statements: matching },
                },
              },
            }),
          );
      }
  }

  private answer(
    connection: {
      push(text: string): void;
      subscriptions: Map<string, { topics: Set<string>; seen: Set<string> }>;
    },
    request: { id: unknown; method: string; params: unknown[] },
  ): void {
    const reply = (body: object) =>
      connection.push(
        JSON.stringify({ jsonrpc: "2.0", id: request.id, ...body }),
      );
    const error = (message: string) =>
      reply({
        error: { code: 7001, message: `Statement store error: ${message}` },
      });
    switch (request.method) {
      case "statement_subscribeStatement": {
        const filter = request.params[0] as { matchAny?: string[] };
        const id = String(this.nextSubscription++);
        connection.subscriptions.set(id, {
          topics: new Set((filter.matchAny ?? []).map((t) => t.toLowerCase())),
          seen: new Set(),
        });
        reply({ result: id });
        connection.push(
          JSON.stringify({
            jsonrpc: "2.0",
            method: "statement_statement",
            params: {
              subscription: id,
              result: {
                event: "newStatements",
                data: { statements: [], remaining: 0 },
              },
            },
          }),
        );
        return;
      }
      case "statement_unsubscribeStatement":
        reply({
          result: connection.subscriptions.delete(String(request.params[0])),
        });
        return;
      case "statement_submit": {
        const hex = String(request.params[0]);
        const encoded = hexToBytes(hex);
        const { meta } = readStatement(encoded);
        if (meta.undecoded !== undefined || meta.expiry === undefined)
          return error("Error decoding statement");
        if (this.nowSeconds >= Number(BigInt(meta.expiry) >> 32n))
          return reply({
            result: { status: "invalid", reason: "alreadyExpired" },
          });
        if (encoded.length > 1024 * 1024 - 1)
          return reply({
            result: {
              status: "invalid",
              reason: "encodingTooLarge",
              submitted_size: encoded.length,
              max_size: 1024 * 1024 - 1,
            },
          });
        if (meta.proof === "none")
          return reply({ result: { status: "invalid", reason: "noProof" } });
        if (this.peers.connected === 0)
          return error("No connected peers to broadcast the statement to");
        if (this.peers.accepting === 0)
          return error(
            `none of the ${this.peers.connected} connected peers accepted the statement`,
          );
        this.broadcast.push(bytesToHex(encoded));
        return reply({ result: { status: "new" } });
      }
    }
  }
}
