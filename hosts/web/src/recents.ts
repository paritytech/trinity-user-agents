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
 * The products a wallet opened, newest first, kept per wallet in this
 * browser. It holds addresses and product ids only, never a recovery phrase,
 * and skips addresses that look like they carry a secret.
 */
export class RecentProducts {
  constructor(
    private readonly storage: Pick<
      Storage,
      "getItem" | "setItem" | "removeItem"
    >,
  ) {}

  list(scope: string): RecentEntry[] {
    try {
      const parsed: unknown = JSON.parse(
        this.storage.getItem(KEY_PREFIX + scope) ?? "[]",
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
    now: number = Date.now(),
  ): boolean {
    if (!isRecordable(entry.address)) return false;
    const rest = this.list(scope).filter(
      (item) =>
        item.address !== entry.address || item.productId !== entry.productId,
    );
    try {
      this.storage.setItem(
        KEY_PREFIX + scope,
        JSON.stringify([{ ...entry, at: now }, ...rest].slice(0, RECENT_LIMIT)),
      );
      return true;
    } catch {
      return false;
    }
  }

  /** Drop a wallet's history, as when the wallet is forgotten. */
  forget(scope: string): void {
    this.storage.removeItem(KEY_PREFIX + scope);
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
