import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { equalBytes } from "./codec.js";
import { decodeStatement } from "./statement.js";

/**
 * A statement store behind JSON-RPC, in memory, for tests. It answers the
 * three methods {@link StatementStore} calls, in the node's shapes: a
 * subscription gets what is stored and what comes later as `newStatements`
 * notifications, and a newer statement from the same signer on the same
 * channel replaces the older one. Nothing leaves the process.
 */

type Filter = { matchAll?: string[]; matchAny?: string[] };

interface Subscriber {
  filter: Filter;
  push: (text: string) => void;
}

interface Request {
  id: number;
  method: string;
  params: unknown[];
}

export class MemoryStatementRpc {
  statements: Uint8Array[] = [];
  private readonly subscribers = new Map<string, Subscriber>();
  private nextSubscription = 1;

  private matches(filter: Filter, encoded: Uint8Array): boolean {
    const topics: string[] = decodeStatement(encoded).topics.map(bytesToHex);
    if (filter.matchAll !== undefined)
      return filter.matchAll.every((topic) => topics.includes(topic));
    return (filter.matchAny ?? []).some((topic) => topics.includes(topic));
  }

  private notify(subscription: string, statements: Uint8Array[]): string {
    return JSON.stringify({
      jsonrpc: "2.0",
      method: "statement_statement",
      params: {
        subscription,
        result: {
          event: "newStatements",
          data: { statements: statements.map(bytesToHex), remaining: 0 },
        },
      },
    });
  }

  private store(encoded: Uint8Array): void {
    const incoming = decodeStatement(encoded);
    this.statements = this.statements.filter((stored) => {
      const old = decodeStatement(stored);
      return !(
        incoming.channel !== null &&
        old.channel !== null &&
        equalBytes(old.channel, incoming.channel) &&
        equalBytes(old.signer, incoming.signer)
      );
    });
    this.statements.push(encoded);
    for (const [subscription, { filter, push }] of this.subscribers)
      if (this.matches(filter, encoded))
        push(this.notify(subscription, [encoded]));
  }

  /** A connection, as the light client's JSON-RPC connection would be. */
  connect(): JsonRpcConnection {
    const queue: string[] = [];
    let wake: (() => void) | null = null;
    let open = true;
    const own = new Set<string>();
    const push = (text: string) => {
      if (!open) return;
      queue.push(text);
      wake?.();
    };
    const later = (text: string) => queueMicrotask(() => push(text));
    const answer = (id: number, result: unknown) =>
      later(JSON.stringify({ jsonrpc: "2.0", id, result }));
    return {
      send: (text) => {
        const { id, method, params } = JSON.parse(text) as Request;
        switch (method) {
          case "statement_subscribeStatement": {
            const subscription = `sub-${this.nextSubscription++}`;
            const filter = params[0] as Filter;
            this.subscribers.set(subscription, { filter, push: later });
            own.add(subscription);
            answer(id, subscription);
            const stored = this.statements.filter((encoded) =>
              this.matches(filter, encoded),
            );
            if (stored.length > 0) later(this.notify(subscription, stored));
            return;
          }
          case "statement_unsubscribeStatement":
            this.subscribers.delete(params[0] as string);
            own.delete(params[0] as string);
            answer(id, true);
            return;
          case "statement_submit":
            answer(id, { status: "new" });
            queueMicrotask(() =>
              this.store(hexToBytes(params[0] as `0x${string}`)),
            );
            return;
          default:
            later(
              JSON.stringify({
                jsonrpc: "2.0",
                id,
                error: { message: `no method ${method}` },
              }),
            );
        }
      },
      responses: () => ({
        [Symbol.asyncIterator]: () => ({
          next: async (): Promise<IteratorResult<string>> => {
            while (queue.length === 0 && open)
              await new Promise<void>((resolve) => (wake = resolve));
            wake = null;
            return queue.length > 0
              ? { value: queue.shift()!, done: false }
              : { value: undefined, done: true };
          },
        }),
      }),
      close: () => {
        open = false;
        for (const subscription of own) this.subscribers.delete(subscription);
        wake?.();
      },
    };
  }
}
