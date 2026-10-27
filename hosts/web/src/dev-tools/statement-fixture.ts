import { hexBytes } from "../invitations/contract.js";
import { digest, toHex } from "../invitations/sealing.js";
import type { SharedStore } from "./shared-store.js";

/**
 * DEV ONLY. A STATEMENT STORE STAND-IN for test users, who reach no chain.
 * Example Chat posts to and watches topics with it, where a product would use
 * TrUAPI's `statementStore`. The messages share the invitations port but are
 * not part of the invitation API: they carry a `statements` field instead of
 * a `call`.
 *
 * Statements are kept per network in a store every tab of this origin reads,
 * like a public store, and lapse after a while. Simulated: no chat account
 * signs a statement, no proof or allowance is needed, and anyone of this
 * origin can post to any topic.
 */

/** How long a statement stays readable. A dev choice. */
export const STATEMENT_LIFETIME_MS = 7 * 24 * 60 * 60_000;
const DATA_MAX_BYTES = 16_384;

/** Product to host. */
export type StatementCall =
  | { statements: "subscribe"; topic: string }
  | { statements: "post"; topic: string; data: string };

/** Host to product. */
export interface StatementMessage {
  statements: "message";
  topic: string;
  data: string;
}

export function isStatementCall(value: unknown): value is StatementCall {
  if (typeof value !== "object" || value === null) return false;
  const call = value as Record<string, unknown>;
  if (hexBytes(call.topic) !== 32) return false;
  if (call.statements === "subscribe") return true;
  const bytes = hexBytes(call.data);
  return (
    call.statements === "post" &&
    bytes !== null &&
    bytes > 0 &&
    bytes <= DATA_MAX_BYTES
  );
}

interface Stored {
  data: string;
  storedUntil: number;
}

export interface StatementFixtureOptions {
  network: string;
  store: SharedStore;
  now(): number;
  /** Hand a statement to the product. */
  send(message: StatementMessage): void;
  lifetimeMs?: number;
}

export class StatementFixture {
  private readonly topics = new Set<string>();
  private readonly prefix: string;
  private readonly unlisten: () => void;

  constructor(private readonly options: StatementFixtureOptions) {
    this.prefix = `truapi-web-signing-host:network:${options.network}:dev:statements:`;
    this.unlisten = options.store.onChange((key) => {
      if (key === null) for (const topic of this.topics) this.replay(topic);
      else if (key.startsWith(this.prefix)) this.deliver(key);
    });
  }

  call(call: StatementCall): void {
    if (call.statements === "subscribe") {
      this.topics.add(call.topic);
      this.replay(call.topic);
      return;
    }
    const id = toHex(digest("statement", call.topic, call.data)).slice(2, 34);
    this.options.store.set(
      `${this.prefix}${call.topic}:${id}`,
      JSON.stringify({
        data: call.data,
        storedUntil:
          this.options.now() +
          (this.options.lifetimeMs ?? STATEMENT_LIFETIME_MS),
      } satisfies Stored),
    );
  }

  /** Forget this product's topics. Statements stay. */
  reset(): void {
    this.topics.clear();
  }

  close(): void {
    this.reset();
    this.unlisten();
  }

  private replay(topic: string): void {
    for (const key of this.options.store.keys(`${this.prefix}${topic}:`))
      this.deliver(key);
  }

  private deliver(key: string): void {
    const topic = key.slice(this.prefix.length).split(":")[0]!;
    if (!this.topics.has(topic)) return;
    let stored: Partial<Stored>;
    try {
      stored = JSON.parse(this.options.store.get(key) ?? "null") as Stored;
    } catch {
      return;
    }
    if (
      typeof stored?.data !== "string" ||
      typeof stored.storedUntil !== "number" ||
      stored.storedUntil <= this.options.now()
    )
      return;
    this.options.send({ statements: "message", topic, data: stored.data });
  }
}
