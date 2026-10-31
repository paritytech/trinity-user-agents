import { hexToBytes } from "@parity/truapi/scale";
import type { ChainDirectory } from "../chat/nova/directory.js";
import type { HandleNames } from "../contacts/contacts.js";
import type { ContactHandle } from "./contract.js";
import type { ContactService, Person } from "./ports.js";
import { publishedChatKey } from "./published-keys.js";
import { toHex } from "./sealing.js";

/**
 * The request service's people (`ContactService`) over a chain directory:
 * each person is their identity account, with the X25519 chat key the
 * directory publishes for it, never a key a request names. A person whose key
 * is missing, conflicting or unreadable can be named, but not reached or
 * authenticated, and the host says why. Handles are the core's, through the
 * host's handle index.
 *
 * Who a person is to this host, and whether it takes anything from them, is
 * the caller's ({@link DirectoryContactsDeps.person}). Naming someone is not
 * permission: whom a transaction may pay is the host's contacts lookup alone.
 */

/** Who the person with an identity account is to this host. Host only. */
export interface KnownPerson {
  /** Keys this host's lists of them; stable for the person. */
  id: string;
  name: string;
  /** Shown as their username, and seeds their avatar. */
  handle: string;
}

export interface DirectoryContactsDeps {
  handles: HandleNames;
  directory: ChainDirectory;
  /**
   * Who the identity account `account` is, or undefined when this host takes
   * nothing from them. `published` is the username the chain records give
   * the account, read with its chat key, or null before that or when none.
   */
  person(account: string, published: string | null): KnownPerson | undefined;
  log?(line: string): void;
}

/**
 * A key read, with the username read beside it, or why there is none;
 * `again` when the chain could not be read, so asking reads again.
 */
type Key =
  | { chatKey: Uint8Array; name: string | null }
  | { problem: string; again: boolean };

export class DirectoryContacts implements ContactService {
  /** Each identity's published chat key, or why there is none, by lower-case account. */
  private readonly keys = new Map<string, Key>();
  /** Reads under way, so two asks for one account read it once. */
  private readonly reading = new Map<string, Promise<void>>();
  private readonly listeners = new Set<() => void>();

  constructor(private readonly deps: DirectoryContactsDeps) {}

  /** Who `account` is, with the username read beside their chat key. */
  private person(account: string): KnownPerson | undefined {
    const key = this.keys.get(account.toLowerCase());
    return this.deps.person(
      account,
      key !== undefined && "chatKey" in key ? key.name : null,
    );
  }

  /** Read the published chat keys of `accounts` again. */
  async readKeys(accounts: readonly string[]): Promise<void> {
    await Promise.all(accounts.map((account) => this.read(account, true)));
  }

  async learn(
    target: { identity: string } | { contact: ContactHandle },
  ): Promise<void> {
    const account =
      "identity" in target
        ? target.identity
        : this.deps.handles.accountFor(target.contact.bytes);
    if (account !== null) await this.read(account, false);
  }

  resolve(contact: ContactHandle): Person | null {
    const account = this.deps.handles.accountFor(contact.bytes);
    return account === null ? null : (this.byAccount(account) ?? null);
  }

  problem(contact: ContactHandle): string | null {
    const account = this.deps.handles.accountFor(contact.bytes);
    const known = account === null ? undefined : this.person(account);
    if (account === null || known === undefined) return null;
    const key = this.keys.get(account.toLowerCase());
    if (key === undefined) return `${known.name}'s chat key is not read yet.`;
    return "problem" in key ? `${known.name}: ${key.problem}.` : null;
  }

  byIdentity(signingPublic: string): Person | undefined {
    return this.byAccount(signingPublic);
  }

  /** The person with identity account `account`, once their chat key is read. */
  byAccount(account: string): Person | undefined {
    const known = this.person(account);
    const key = this.keys.get(account.toLowerCase());
    if (known === undefined || key === undefined || "problem" in key) return;
    return {
      id: known.id,
      account,
      name: known.name,
      handle: known.handle,
      identity: {
        signingPublic: account.toLowerCase(),
        agreementPublic: toHex(key.chatKey),
      },
    };
  }

  /** Who `account` is, with or without a chat key: enough to draw them. */
  nameOf(account: string): KnownPerson | undefined {
    return this.person(account);
  }

  handleFor(person: Person): ContactHandle | null {
    return this.handleForIdentity(person.account);
  }

  handleForIdentity(signingPublic: string): ContactHandle | null {
    if (this.person(signingPublic) === undefined) return null;
    const bytes = this.deps.handles.handleFor(signingPublic);
    return bytes === null ? null : { bytes };
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    const stopHandles = this.deps.handles.onChange(listener);
    return () => {
      this.listeners.delete(listener);
      stopHandles();
    };
  }

  /**
   * Read one account's key, unless `again` is false and it is read already:
   * a key, or a definite answer that there is none. A read that failed is
   * read again whenever it is asked for.
   */
  private read(account: string, again: boolean): Promise<void> {
    const key = account.toLowerCase();
    const known = this.person(account);
    const cached = this.keys.get(key);
    const settled =
      cached !== undefined && !("problem" in cached && cached.again);
    if (known === undefined || (!again && settled)) return Promise.resolve();
    const under = this.reading.get(key);
    if (under !== undefined) return under;
    const reading = publishedChatKey(this.deps.directory, hexToBytes(account))
      .then((published) => {
        if (published.ok)
          this.keys.set(key, {
            chatKey: published.chatKey,
            name: published.name,
          });
        else {
          this.keys.set(key, {
            problem: published.reason,
            again: published.unread,
          });
          this.deps.log?.(`Cannot reach ${known.name}: ${published.reason}.`);
        }
        for (const listener of [...this.listeners]) listener();
      })
      .finally(() => this.reading.delete(key));
    this.reading.set(key, reading);
    return reading;
  }
}
