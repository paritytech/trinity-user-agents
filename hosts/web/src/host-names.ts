import { hexToBytes } from "@parity/truapi/scale";
import { lookupChatIdentity, type ChainDirectory } from "./chat/directory.js";
import { loadChatRecord } from "./chat/store.js";

/**
 * The host's own names for people, by account, for rooms a chat product
 * keeps: the product says only whom a room is with, and the host names them.
 *
 * Names come, in order, from what the user chose in the host's picker this
 * session, from the host's own chat record, and from the username the chain
 * holds for the account, read once per session. None of them comes from a
 * product, and none is given to one.
 */

export interface HostNamesSession {
  /** Names one wallet on one network; the same scope again keeps every name. */
  scope: string;
  /** The host's own chat record, as JSON, or null. */
  record(): string | null;
  /** Chain reads for usernames; null when the chain is not connected. */
  directory(): ChainDirectory | null;
}

const key = (account: string): string => account.toLowerCase();

export class HostNames {
  private session: HostNamesSession | null = null;
  private generation = 0;
  private readonly chosen = new Map<string, string>();
  private readonly recorded = new Map<string, string>();
  private readonly onChain = new Map<string, string | null>();
  private readonly listeners = new Set<() => void>();

  /** Follow the signed-in session. A new one forgets every name. */
  setSession(session: HostNamesSession | null): void {
    if (session?.scope === this.session?.scope) return;
    this.generation += 1;
    this.session = session;
    this.chosen.clear();
    this.recorded.clear();
    this.onChain.clear();
    if (session === null) return;
    const raw = session.record();
    if (raw === null) return;
    const readOnly: Pick<Storage, "getItem" | "setItem"> = {
      getItem: () => raw,
      setItem: () => {},
    };
    const record = loadChatRecord(readOnly as Storage, "");
    for (const entry of [...record.contacts, ...record.left])
      if (entry.name !== null)
        this.recorded.set(key(entry.account), entry.name);
    for (const request of record.requests)
      if (request.name !== null)
        this.recorded.set(key(request.peer), request.name);
  }

  /** Name `account` as the user saw them in the picker, for this session. */
  remember(account: string, name: string): void {
    this.chosen.set(key(account), name);
    this.changed();
  }

  /** The name the host has for `account`, or null; asks the chain once when it has none. */
  name(account: string): string | null {
    const id = key(account);
    const known =
      this.chosen.get(id) ?? this.recorded.get(id) ?? this.onChain.get(id);
    if (known !== undefined) return known;
    this.lookUp(id);
    return null;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private changed(): void {
    for (const listener of [...this.listeners]) listener();
  }

  private lookUp(account: string): void {
    const directory = this.session?.directory() ?? null;
    if (directory === null || !/^0x[0-9a-f]{64}$/.test(account)) return;
    const generation = this.generation;
    this.onChain.set(account, null);
    void lookupChatIdentity(directory, hexToBytes(account))
      .then((identity) => identity?.name ?? null)
      .catch(() => null)
      .then((name) => {
        if (generation !== this.generation) return;
        this.onChain.set(account, name);
        if (name !== null) this.changed();
      });
  }
}
