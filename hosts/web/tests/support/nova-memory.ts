import { equalBytes } from "../../src/chat/nova/codec.js";
import type {
  AccountNames,
  ChainDirectory,
  PeopleConsumer,
} from "../../src/chat/nova/directory.js";
import type { StatementStoreLike } from "../../src/chat/nova/engine.js";
import { decodeStatement } from "../../src/chat/nova/statement.js";
import type {
  SubmitResult,
  TopicFilter,
} from "../../src/chat/nova/statement-store.js";

/**
 * A statement store in memory with the store's own rules: a subscription gets
 * what is stored and what comes later, and a newer statement from the same
 * signer on the same channel replaces the older one.
 */
export class MemoryStatementStore {
  statements: Uint8Array[] = [];
  /** Every statement submitted, including those a later one replaced. */
  submitted: Uint8Array[] = [];
  private readonly subscribers = new Set<{
    filter: TopicFilter;
    deliver: (s: Uint8Array[]) => void;
  }>();

  private matches(filter: TopicFilter, encoded: Uint8Array): boolean {
    const topics = decodeStatement(encoded).topics;
    const has = (topic: Uint8Array) =>
      topics.some((candidate) => equalBytes(candidate, topic));
    return "matchAll" in filter
      ? filter.matchAll.every(has)
      : filter.matchAny.some(has);
  }

  /** Put a statement in the store as if a peer had submitted it. */
  inject(encoded: Uint8Array): void {
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
    for (const { filter, deliver } of this.subscribers)
      if (this.matches(filter, encoded))
        queueMicrotask(() => deliver([encoded]));
  }

  connect(allowed = true): StatementStoreLike {
    const own = new Set<{
      filter: TopicFilter;
      deliver: (s: Uint8Array[]) => void;
    }>();
    let open = true;
    let ended!: () => void;
    const closed = new Promise<void>((resolve) => (ended = resolve));
    const end = () => {
      if (!open) return;
      open = false;
      for (const subscriber of own) this.subscribers.delete(subscriber);
      ended();
    };
    return {
      closed,
      subscribe: async (filter, deliver) => {
        if (!open) throw new Error("closed");
        const subscriber = { filter, deliver };
        this.subscribers.add(subscriber);
        own.add(subscriber);
        const stored = this.statements.filter((encoded) =>
          this.matches(filter, encoded),
        );
        if (stored.length > 0) queueMicrotask(() => deliver(stored));
        return () => {
          this.subscribers.delete(subscriber);
          own.delete(subscriber);
        };
      },
      submit: async (encoded): Promise<SubmitResult> => {
        if (!open) throw new Error("closed");
        if (!allowed) return { status: "rejected", reason: "noAllowance" };
        this.submitted.push(encoded);
        this.inject(encoded);
        return { status: "new" };
      },
      close: end,
    };
  }
}

/** A person the memory directory knows: public keys only. */
export interface DirectoryPerson {
  label: string;
  identity: Uint8Array;
  chatKey: Uint8Array;
  allowance?: boolean;
}

/** A chain directory over people registered on Asset Hub. */
export function memoryDirectory(people: DirectoryPerson[]): ChainDirectory {
  const find = (account: Uint8Array) =>
    people.find((person) => equalBytes(person.identity, account));
  return {
    async names(account): Promise<AccountNames | null> {
      const person = find(account);
      return person === undefined
        ? null
        : {
            lite: { label: person.label, chatKey: person.chatKey },
            full: null,
          };
    },
    async consumer(): Promise<PeopleConsumer | null> {
      return null;
    },
    async owners(label) {
      const person = people.find((candidate) => candidate.label === label);
      return { assetHub: person?.identity ?? null, people: null };
    },
    async hasAllowance(account) {
      return find(account)?.allowance ?? true;
    },
  };
}

export const settle = () => new Promise((resolve) => setTimeout(resolve, 20));
