import { DEFAULT_NETWORK, normalizeNetwork } from "./network-scope.js";

/** One product opened before, with everything needed to open it the same way. */
export interface RecentEntry {
  /** The canonical address: a name with its TLD, or a full URL. */
  address: string;
  /** The product id it opened as. */
  productId: string;
  /** Whether that id was entered, not derived from the address. */
  entered: boolean;
  /** Milliseconds since the epoch. */
  at: number;
}

/** The most entries kept for one wallet. */
export const RECENT_LIMIT = 12;

const KEY_PREFIX = "truapi-web-signing-host:recents:v1:";

/**
 * The storage key one network keeps a scope's history under. A network other
 * than the default gets a `network:<id>:` segment; the default keeps the
 * original key.
 */
function keyFor(scope: string, network: string): string {
  const id = normalizeNetwork(network);
  return id === DEFAULT_NETWORK
    ? KEY_PREFIX + scope
    : `truapi-web-signing-host:network:${id}:recents:v1:${scope}`;
}

const CREDENTIALS = /^[a-z][a-z0-9+.-]*:\/\/[^/?#]*@/i;
const SENSITIVE_PARAM =
  /[?&#;](?:access[_-]?token|id[_-]?token|refresh[_-]?token|token|auth|authorization|api[_-]?key|key|secret|password|passwd|pwd|mnemonic|seed|phrase|session|sid|jwt|code|signature|sig)(?:=|:|&|$)/i;

/**
 * Whether an address is safe to keep in the history. Credentials in the
 * address, and query or hash parameters named like secrets, keep it out. The
 * address still opens as typed; it is only not remembered.
 */
export function isRecordable(address: string): boolean {
  return !CREDENTIALS.test(address) && !SENSITIVE_PARAM.test(address);
}

function isEntry(value: unknown): value is RecentEntry {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return (
    typeof entry.address === "string" &&
    typeof entry.productId === "string" &&
    typeof entry.entered === "boolean" &&
    typeof entry.at === "number"
  );
}

/**
 * The products a wallet opened, newest first, kept per wallet and network in
 * this browser. It holds addresses and product ids only, never a recovery
 * phrase, and skips addresses that look like they carry a secret.
 */
export class RecentProducts {
  constructor(
    private readonly storage: Pick<
      Storage,
      "getItem" | "setItem" | "removeItem" | "length" | "key"
    >,
  ) {}

  list(scope: string, network: string = DEFAULT_NETWORK): RecentEntry[] {
    try {
      const parsed: unknown = JSON.parse(
        this.storage.getItem(keyFor(scope, network)) ?? "[]",
      );
      return Array.isArray(parsed)
        ? parsed.filter(isEntry).slice(0, RECENT_LIMIT)
        : [];
    } catch {
      return [];
    }
  }

  /** Put an entry first, replacing one for the same address and id. False when it was not kept. */
  record(
    scope: string,
    entry: Omit<RecentEntry, "at">,
    network: string = DEFAULT_NETWORK,
    now: number = Date.now(),
  ): boolean {
    if (!isRecordable(entry.address)) return false;
    const rest = this.list(scope, network).filter(
      (item) =>
        item.address !== entry.address || item.productId !== entry.productId,
    );
    try {
      this.storage.setItem(
        keyFor(scope, network),
        JSON.stringify([{ ...entry, at: now }, ...rest].slice(0, RECENT_LIMIT)),
      );
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Drop a scope's history on every network, as when the wallet is forgotten.
   */
  forget(scope: string): void {
    const suffix = `recents:v1:${scope}`;
    for (const key of this.keys())
      if (key.endsWith(suffix)) this.storage.removeItem(key);
  }

  private keys(): string[] {
    const keys: string[] = [];
    for (let index = 0; index < this.storage.length; index += 1) {
      const key = this.storage.key(index);
      if (key !== null) keys.push(key);
    }
    return keys;
  }
}

/** The entries that match what was typed, by address or product id. All of them for empty text. */
export function matchRecents(
  entries: RecentEntry[],
  typed: string,
): RecentEntry[] {
  const needle = typed.trim().toLowerCase();
  return needle === ""
    ? entries
    : entries.filter(
        (entry) =>
          entry.address.toLowerCase().includes(needle) ||
          entry.productId.toLowerCase().includes(needle),
      );
}
