import type { HexString } from "@parity/truapi/scale";
import type { ChatContact } from "../chat/provider.js";

/**
 * The people a wallet keeps in this browser, read from its saved chat record:
 * one record per network and wallet in `localStorage`, under
 * {@link walletRecordKey}. The host writes the record only to block or
 * unblock one of its contacts, as the host's chat wrote it: `blocked: true`
 * on that contact, or no `blocked` at all. Nothing else in it changes.
 *
 * - Contacts are the accepted people who are not blocked. They are what the
 *   contact picker offers a product, and so the only people a transaction
 *   resolves a picked handle to. People the user left are not in the
 *   record's contacts.
 * - Blocked people are never offered, picked or answered.
 * - Names are the ones the record holds for its contacts, the people the user
 *   left and the people who asked to chat.
 */

/** Where a wallet's saved chat record is, per network. */
export const walletRecordKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:chat:${walletId}:v1`;

interface RecordedPerson {
  account: HexString;
  name: string | null;
}

interface RecordedContact extends RecordedPerson {
  /** `requested`: a request went out and no answer came yet. */
  state: string;
  blocked?: boolean;
  messages?: { at: number }[];
}

/** What the host reads of a saved chat record. */
export interface WalletRecord {
  contacts: RecordedContact[];
  requests: { peer: HexString; name: string | null }[];
  left: RecordedPerson[];
}

const EMPTY: WalletRecord = { contacts: [], requests: [], left: [] };

/** The record in `raw`; one that does not read is empty. */
export function readWalletRecord(raw: string | null): WalletRecord {
  if (raw === null) return EMPTY;
  try {
    const parsed = JSON.parse(raw) as Partial<WalletRecord> | null;
    if (!Array.isArray(parsed?.contacts) || !Array.isArray(parsed.requests))
      return EMPTY;
    return {
      contacts: parsed.contacts,
      requests: parsed.requests,
      left: Array.isArray(parsed.left) ? parsed.left : [],
    };
  } catch {
    return EMPTY;
  }
}

/** An account shortened for a name line, when the record holds no name. */
const abbreviate = (account: string): string =>
  `${account.slice(0, 8)}…${account.slice(-6)}`;

/** The accepted people of `record` who are not blocked, in the record's order. */
export function contactsOf(record: WalletRecord): ChatContact[] {
  return record.contacts
    .filter((contact) => contact.state === "active" && !contact.blocked)
    .map((contact) => {
      const last = contact.messages?.at(-1)?.at;
      return {
        account: contact.account,
        name: contact.name ?? abbreviate(contact.account),
        ...(last === undefined ? {} : { lastActiveAt: last }),
      };
    });
}

/** The people `record` holds as blocked, named or shortened. */
export const blockedOf = (record: WalletRecord): ChatContact[] =>
  record.contacts
    .filter((contact) => contact.blocked === true)
    .map((contact) => ({
      account: contact.account,
      name: contact.name ?? abbreviate(contact.account),
    }));

/**
 * `raw` with the contact `account` blocked or not, every other field as it
 * was, or null when the record holds no such contact or it is already so.
 */
export function withBlock(
  raw: string | null,
  account: string,
  blocked: boolean,
): string | null {
  if (raw === null) return null;
  let parsed: { contacts?: unknown };
  try {
    parsed = JSON.parse(raw) as { contacts?: unknown };
  } catch {
    return null;
  }
  if (!Array.isArray(parsed?.contacts)) return null;
  const contact = (parsed.contacts as Record<string, unknown>[]).find(
    (entry) =>
      typeof entry?.account === "string" &&
      entry.account.toLowerCase() === account.toLowerCase(),
  );
  if (contact === undefined || (contact.blocked === true) === blocked)
    return null;
  if (blocked) contact.blocked = true;
  else delete contact.blocked;
  return JSON.stringify(parsed);
}

/** The names `record` holds, by lower-case account. */
export function namesOf(record: WalletRecord): Map<string, string> {
  const names = new Map<string, string>();
  for (const person of [...record.contacts, ...record.left])
    if (person.name !== null)
      names.set(person.account.toLowerCase(), person.name);
  for (const request of record.requests)
    if (request.name !== null)
      names.set(request.peer.toLowerCase(), request.name);
  return names;
}

/** The signed-in wallet's record, and whether its contacts are offered. */
export interface WalletContactsSession {
  /** Where the record is: {@link walletRecordKey}. */
  key: string;
  /** Names one session: the same scope again changes nothing. */
  scope: string;
  /**
   * Whether the picker may offer the contacts, and the user block or unblock
   * one, read once per scope. Blocked people are refused either way.
   */
  offered(): boolean;
}

/** The signed-in wallet's saved contacts and blocked people. */
export class WalletContacts {
  private session: { key: string; scope: string; offered: boolean } | null =
    null;
  private readonly listeners = new Set<() => void>();

  constructor(private readonly storage: Pick<Storage, "getItem" | "setItem">) {}

  /** Follow the signed-in wallet, or none. */
  setSession(session: WalletContactsSession | null): void {
    if (session?.scope === this.session?.scope) return;
    this.session =
      session === null
        ? null
        : {
            key: session.key,
            scope: session.scope,
            offered: session.offered(),
          };
    for (const listener of [...this.listeners]) listener();
  }

  contacts(): ChatContact[] {
    return this.session?.offered === true ? contactsOf(this.record()) : [];
  }

  /** The accounts never offered, picked or answered. */
  blocked(): HexString[] {
    return this.session === null
      ? []
      : blockedOf(this.record()).map((person) => person.account);
  }

  /** The blocked people the user may unblock now. */
  blockedPeople(): ChatContact[] {
    return this.session?.offered === true ? blockedOf(this.record()) : [];
  }

  /** The record's contacts the user may block now, in any state. */
  blockable(): ChatContact[] {
    if (this.session?.offered !== true) return [];
    return this.record()
      .contacts.filter((contact) => contact.blocked !== true)
      .map((contact) => ({
        account: contact.account,
        name: contact.name ?? abbreviate(contact.account),
      }));
  }

  /** Whether `account` is a contact of the record the user may block now. */
  canBlock(account: string): boolean {
    return (
      this.session?.offered === true &&
      withBlock(this.storage.getItem(this.session.key), account, true) !== null
    );
  }

  /**
   * Block the contact `account`: never offered, picked or answered until
   * unblocked. The person is not told.
   */
  block(account: string): void {
    this.setBlocked(account, true);
  }

  /** Unblock `account`: they are offered again when they are an accepted contact. */
  unblock(account: string): void {
    this.setBlocked(account, false);
  }

  private setBlocked(account: string, blocked: boolean): void {
    const session = this.session;
    if (session?.offered !== true) return;
    const next = withBlock(this.storage.getItem(session.key), account, blocked);
    if (next === null) return;
    this.storage.setItem(session.key, next);
    for (const listener of [...this.listeners]) listener();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private record(): WalletRecord {
    return this.session === null
      ? EMPTY
      : readWalletRecord(this.storage.getItem(this.session.key));
  }
}
