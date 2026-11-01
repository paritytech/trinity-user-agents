import * as sr25519 from "@scure/sr25519";
import { signingPayload } from "../../src/platform/statement.js";
import { blake2b256, sr25519Verify } from "../../src/platform/crypto.js";
import { concat, fromHex, toHex, utf8 } from "../../src/invitations/sealing.js";
import type { ConversationAllowance } from "../../src/invitations/ports.js";
import type {
  Heard,
  Post,
} from "../../fixtures/products/contact-chat/protocol.js";
import type { Statements } from "../../fixtures/products/contact-chat/transport.js";

/**
 * A Statement Store in memory as Contact Chat's products reach it through
 * TrUAPI, for tests, with the node's rules for one account
 * (`substrate/client/statement-store/src/lib.rs` at polkadot-sdk
 * 06ccd162f): a statement must be signed by an account an allowance covers
 * and not be expired; a statement on a channel the account already holds
 * replaces it only with a strictly higher expiry; past the allowance's
 * statement count, the lowest expiry is evicted, and a statement lower than
 * all it would have to evict is refused. A follower gets what is held on its
 * topics, then what comes later. Nothing leaves the process.
 *
 * A product's account at an index stands in for the core's derivation: a key
 * from the product id and the index, which only this double holds.
 */

/** A statement as the store holds it. */
export interface Held {
  signer: string;
  expiry: bigint;
  channel: string;
  topics: string[];
  data: string;
}

/** What one account's allowance lets it keep, as the live People chain grants it (2026-10-30). */
const MAX_COUNT = 2;
const MAX_BYTES = 512_000;

export class MemoryChatStatements {
  /** What the store holds, in the order it took it. */
  held: Held[] = [];
  /** Every statement the store took, including those it later replaced. */
  readonly taken: Held[] = [];
  /** Accounts an allowance covers, `0x` hex. */
  readonly allowed = new Set<string>();
  /** Every (product, index) a host asked to allow, in order. */
  readonly asked: { product: string; index: string }[] = [];
  /** The store's clock, in seconds. */
  now = () => Math.floor(Date.now() / 1000);
  private readonly followers = new Set<{
    topics: string[];
    deliver: (heard: Heard[]) => void;
  }>();

  /** How many follow a topic now. */
  followersOf(topic: string): number {
    return [...this.followers].filter(({ topics }) => topics.includes(topic))
      .length;
  }

  private static secret(product: string, index: string): Uint8Array {
    return sr25519.secretFromSeed(
      blake2b256(concat(utf8(`${product}/`), fromHex(index))),
    );
  }

  /** The account a product has at `index`. */
  static account(product: string, index: string): string {
    return toHex(
      sr25519.getPublicKey(MemoryChatStatements.secret(product, index)),
    );
  }

  private static heard(held: Held): Heard {
    return { signer: held.signer, topics: held.topics, data: held.data };
  }

  private take(statement: Held, signature: Uint8Array): void {
    const payload = signingPayload({
      expiry: statement.expiry,
      channel: fromHex(statement.channel),
      topics: statement.topics.map(fromHex),
      data: fromHex(statement.data),
    });
    if (!sr25519Verify(payload, signature, fromHex(statement.signer)))
      throw new Error("statement-store submit failed: bad proof");
    if (!this.allowed.has(statement.signer))
      throw new Error("statement-store submit failed: no allowance");
    if (statement.expiry >> 32n <= BigInt(this.now()))
      throw new Error("statement-store submit failed: already expired");
    const own = this.held.filter((held) => held.signer === statement.signer);
    const replaced = own.find((held) => held.channel === statement.channel);
    if (replaced !== undefined && statement.expiry <= replaced.expiry)
      throw new Error(
        "statement-store submit failed: channel priority too low",
      );
    const kept = own
      .filter((held) => held !== replaced)
      .sort((left, right) => (left.expiry < right.expiry ? -1 : 1));
    const bytes = (list: Held[]) =>
      list.reduce((sum, held) => sum + (held.data.length - 2) / 2, 0);
    const size = (statement.data.length - 2) / 2;
    while (kept.length + 1 > MAX_COUNT || bytes(kept) + size > MAX_BYTES) {
      if (kept[0]!.expiry >= statement.expiry)
        throw new Error("statement-store submit failed: account full");
      kept.shift();
    }
    this.held = [
      ...this.held.filter((held) => held.signer !== statement.signer),
      ...kept,
      statement,
    ];
    this.taken.push(statement);
    for (const { topics, deliver } of [...this.followers])
      if (statement.topics.some((topic) => topics.includes(topic)))
        queueMicrotask(() => deliver([MemoryChatStatements.heard(statement)]));
  }

  /** One product's end. */
  statements(product: string): Statements {
    return {
      account: async (index) => MemoryChatStatements.account(product, index),
      follow: (topics, deliver) => {
        const follower = { topics, deliver };
        this.followers.add(follower);
        const held = this.held.filter((statement) =>
          statement.topics.some((topic) => topics.includes(topic)),
        );
        queueMicrotask(() => {
          if (this.followers.has(follower) && held.length > 0)
            deliver(held.map(MemoryChatStatements.heard));
        });
        return () => this.followers.delete(follower);
      },
      post: async (index, post: Post, expiry) => {
        const secret = MemoryChatStatements.secret(product, index);
        const statement: Held = {
          signer: MemoryChatStatements.account(product, index),
          expiry,
          channel: post.channel,
          topics: post.topics,
          data: post.data,
        };
        const signature = sr25519.sign(
          secret,
          signingPayload({
            expiry,
            channel: fromHex(post.channel),
            topics: post.topics.map(fromHex),
            data: fromHex(post.data),
          }),
        );
        this.take(statement, signature);
      },
    };
  }

  /** A host's way to allow accounts, until `until`; `refuse` makes it say why not instead. */
  allowance(
    until: () => number,
    refuse: () => string | null = () => null,
  ): ConversationAllowance {
    return {
      allow: async (product, index) => {
        const hex = toHex(index);
        this.asked.push({ product, index: hex });
        const reason = refuse();
        if (reason !== null) throw new Error(reason);
        const account = MemoryChatStatements.account(product, hex);
        this.allowed.add(account);
        return { account, until: until() };
      },
    };
  }
}
