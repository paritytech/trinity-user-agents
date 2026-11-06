import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type { JsonRpcConnection } from "@parity/truapi-host";
import { equalBytes } from "../../src/platform/codec.js";
import {
  decodeStatement,
  statementHash,
  verifyStatement,
  type SignedStatement,
} from "../../src/platform/statement.js";
import type { SharedStore } from "../invitations/shared-store.js";

/**
 * DEV ONLY. A SIMULATED People chain Statement Store, shared by the tabs of
 * this browser: it answers `statement_subscribeStatement`,
 * `statement_unsubscribeStatement` and `statement_submit` in the node's
 * shapes, from statements kept in a {@link SharedStore}. Every other method
 * goes to the real People connection, opened only when one is asked.
 *
 * The core and the products use it exactly as they use the network's: the
 * core signs and submits, a subscription gets what is stored and then what
 * arrives, from this tab or another.
 *
 * What it checks, as an approximation of the node: the sr25519 proof, an
 * expiry in the future, the replacement rule on one account's channel (a
 * strictly higher expiry wins), and at most {@link LocalStatementNodeOptions.perAccount}
 * statements per account, the one with the lowest expiry evicted for a
 * higher one. It has no allowance, no size limits and no gossip, and anyone
 * who reads this origin's storage reads everything in it.
 */

export interface LocalStatementNodeOptions {
  store: SharedStore;
  /** Names the store's keys, so two networks keep apart. */
  network: string;
  /** Statements one account may keep; the general grant was 2 (read 2026-10-30). */
  perAccount?: number;
  now?(): number;
  /** Told of each submission and its answer. */
  log?(line: string): void;
}

type Filter = { matchAll?: string[]; matchAny?: string[] };

interface Subscriber {
  filter: Filter;
  push(statements: string[]): void;
}

interface Kept {
  encoded: string;
  /** Seconds since the Unix epoch. */
  expiresAt: number;
}

const METHODS = new Set([
  "statement_subscribeStatement",
  "statement_unsubscribeStatement",
  "statement_submit",
]);

export class LocalStatementNode {
  private readonly subscribers = new Map<string, Subscriber>();
  private nextSubscription = 1;
  private readonly prefix: string;
  private readonly perAccount: number;
  private readonly now: () => number;

  constructor(private readonly options: LocalStatementNodeOptions) {
    this.prefix = `truapi-web-signing-host:local-statements:${options.network}:`;
    this.perAccount = options.perAccount ?? 2;
    this.now = options.now ?? Date.now;
    options.store.onChange((key) => {
      if (key === null || !key.startsWith(this.prefix)) return;
      const kept = this.read(key);
      if (kept !== null) this.announce(kept.encoded);
    });
  }

  private read(key: string): Kept | null {
    try {
      const kept = JSON.parse(this.options.store.get(key) ?? "null") as Kept;
      return kept !== null && kept.expiresAt > this.now() / 1000 ? kept : null;
    } catch {
      return null;
    }
  }

  /** Every live statement, with its key. */
  private live(): { key: string; kept: Kept; statement: SignedStatement }[] {
    return this.options.store.keys(this.prefix).flatMap((key) => {
      const kept = this.read(key);
      if (kept === null) {
        this.options.store.remove(key);
        return [];
      }
      return [
        {
          key,
          kept,
          statement: decodeStatement(hexToBytes(kept.encoded as `0x${string}`)),
        },
      ];
    });
  }

  private static matches(filter: Filter, encoded: string): boolean {
    const topics = decodeStatement(
      hexToBytes(encoded as `0x${string}`),
    ).topics.map((topic) => bytesToHex(topic).toLowerCase());
    if (filter.matchAll !== undefined)
      return filter.matchAll.every((topic) =>
        topics.includes(topic.toLowerCase()),
      );
    return (filter.matchAny ?? []).some((topic) =>
      topics.includes(topic.toLowerCase()),
    );
  }

  private announce(encoded: string): void {
    for (const subscriber of this.subscribers.values())
      if (LocalStatementNode.matches(subscriber.filter, encoded))
        subscriber.push([encoded]);
  }

  /** The node's answer to `statement_submit`. */
  submit(encoded: string): { status: string; reason?: string } {
    let statement: SignedStatement;
    try {
      statement = decodeStatement(hexToBytes(encoded as `0x${string}`));
    } catch (error) {
      return {
        status: "invalid",
        reason: `undecodable: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
    if (!verifyStatement(statement))
      return { status: "invalid", reason: "badProof" };
    const expiresAt = Number(statement.expiry >> 32n);
    if (expiresAt <= this.now() / 1000)
      return { status: "invalid", reason: "alreadyExpired" };
    const key = `${this.prefix}${bytesToHex(statementHash(hexToBytes(encoded as `0x${string}`)))}`;
    if (this.options.store.get(key) !== null) return { status: "known" };
    const own = this.live().filter(({ statement: kept }) =>
      equalBytes(kept.signer, statement.signer),
    );
    const replaced =
      statement.channel === null
        ? undefined
        : own.find(
            ({ statement: kept }) =>
              kept.channel !== null &&
              equalBytes(kept.channel, statement.channel!),
          );
    if (replaced !== undefined && replaced.statement.expiry >= statement.expiry)
      return { status: "rejected", reason: "channelPriorityTooLow" };
    const others = own.filter((entry) => entry !== replaced);
    let evicted: (typeof own)[number] | undefined;
    if (others.length >= this.perAccount) {
      evicted = [...others].sort((left, right) =>
        left.statement.expiry < right.statement.expiry ? -1 : 1,
      )[0]!;
      if (evicted.statement.expiry >= statement.expiry)
        return { status: "rejected", reason: "accountFull" };
    }
    if (replaced !== undefined) this.options.store.remove(replaced.key);
    if (evicted !== undefined) this.options.store.remove(evicted.key);
    this.options.store.set(
      key,
      JSON.stringify({ encoded, expiresAt } satisfies Kept),
    );
    this.announce(encoded);
    return { status: "new" };
  }

  /**
   * A People connection: the statement methods are answered here, anything
   * else goes to the connection `openReal` opens, on its first use.
   */
  connect(openReal: () => Promise<JsonRpcConnection>): JsonRpcConnection {
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
    const answer = (id: unknown, result: unknown) =>
      later(JSON.stringify({ jsonrpc: "2.0", id, result }));
    const notify = (subscription: string, statements: string[]) =>
      later(
        JSON.stringify({
          jsonrpc: "2.0",
          method: "statement_statement",
          params: {
            subscription,
            result: {
              event: "newStatements",
              data: { statements, remaining: 0 },
            },
          },
        }),
      );
    let real: Promise<JsonRpcConnection> | null = null;
    const forward = (text: string, id: unknown) => {
      real ??= openReal().then((connection) => {
        void (async () => {
          for await (const response of connection.responses()) push(response);
        })();
        return connection;
      });
      real.then(
        (connection) => connection.send(text),
        (error: unknown) =>
          later(
            JSON.stringify({
              jsonrpc: "2.0",
              id,
              error: {
                code: -32000,
                message: `the People node is not reachable: ${error instanceof Error ? error.message : String(error)}`,
              },
            }),
          ),
      );
    };
    return {
      send: (text) => {
        const { id, method, params } = JSON.parse(text) as {
          id: unknown;
          method: string;
          params: unknown[];
        };
        if (!METHODS.has(method)) return forward(text, id);
        switch (method) {
          case "statement_subscribeStatement": {
            const subscription = `local-statements-${this.nextSubscription++}`;
            const filter = params[0] as Filter;
            this.subscribers.set(subscription, {
              filter,
              push: (statements) => notify(subscription, statements),
            });
            own.add(subscription);
            answer(id, subscription);
            const stored = this.live()
              .map(({ kept }) => kept.encoded)
              .filter((encoded) => LocalStatementNode.matches(filter, encoded));
            if (stored.length > 0) notify(subscription, stored);
            return;
          }
          case "statement_unsubscribeStatement":
            this.subscribers.delete(params[0] as string);
            own.delete(params[0] as string);
            return answer(id, true);
          case "statement_submit": {
            const result = this.submit(params[0] as string);
            this.options.log?.(
              `Simulated Statement Store: a submission was ${result.status}${result.reason === undefined ? "" : ` (${result.reason})`}.`,
            );
            return answer(id, result);
          }
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
        void real?.then(
          (connection) => connection.close(),
          () => {},
        );
        wake?.();
      },
    };
  }
}
