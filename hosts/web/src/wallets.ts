import { mnemonicToEntropy, validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";

/** A wallet this host can activate a session from. */
export interface Wallet {
  /** Stable id; also the namespace this wallet's storage lives under. */
  id: string;
  /** Label shown in the wallet picker. */
  name: string;
  /** BIP-39 entropy the core derives every key from. */
  entropy: Uint8Array;
}

/** A wallet the user imported, as it is persisted. */
interface SavedWallet {
  id: string;
  name: string;
  mnemonic: string;
}

const SAVED_WALLETS_KEY = "truapi-web-signing-host:wallets:v1";

const UNREADABLE = `Saved wallets cannot be read. Nothing was changed. Fix or delete the "${SAVED_WALLETS_KEY}" entry in the browser's storage tools, then reload.`;

/** What the stored list holds: entries to keep, or bytes this store cannot interpret as a list. */
type StoredList = { readable: true; entries: unknown[] } | { readable: false };

/**
 * Wallets imported in this browser. This host never creates a wallet: an
 * account is made, registered and attested elsewhere, then imported here.
 *
 * Recovery phrases are stored in plain text in `storage`, which is this
 * origin's `localStorage`. That is the reason this host is for development
 * wallets only: any script on this origin can read them.
 *
 * Stored data this store cannot use is never rewritten or dropped. An entry
 * that is malformed or holds an invalid phrase is left out of the list and kept
 * in storage as it was; data that is not a list at all makes the store refuse
 * changes until it is fixed by hand.
 */
export class WalletStore {
  constructor(private readonly storage: Storage) {}

  /** Imported wallets that can be used, in the order they were added. */
  list(): Wallet[] {
    const stored = this.read();
    if (!stored.readable) return [];
    return stored.entries.flatMap((entry) => toWallet(entry) ?? []);
  }

  find(id: string): Wallet | undefined {
    return this.list().find((wallet) => wallet.id === id);
  }

  /** Whether a usable wallet with this id is saved. */
  has(id: string): boolean {
    return this.find(id) !== undefined;
  }

  /**
   * What is wrong with the saved wallets, or null when nothing is. The text
   * says what to do about it.
   */
  problem(): string | null {
    const stored = this.read();
    if (!stored.readable) return UNREADABLE;
    const skipped = stored.entries.filter(
      (entry) => toWallet(entry) === null,
    ).length;
    if (skipped === 0) return null;
    return `${skipped} saved ${skipped === 1 ? "wallet is" : "wallets are"} unreadable and left out. ${skipped === 1 ? "It stays" : "They stay"} in storage under "${SAVED_WALLETS_KEY}".`;
  }

  /**
   * Save a recovery phrase and return its wallet. Saving a phrase that is
   * already saved returns the existing wallet, so one phrase keeps one storage
   * namespace however often it is imported.
   */
  save(name: string, mnemonic: string): Wallet {
    const phrase = normalizeMnemonic(mnemonic);
    if (!validateMnemonic(phrase, wordlist)) {
      throw new Error("That is not a valid BIP-39 recovery phrase.");
    }
    const entries = this.readForChange();
    for (const entry of entries) {
      const wallet = toWallet(entry);
      if (wallet && savedWalletOf(entry)?.mnemonic === phrase) return wallet;
    }
    const saved: SavedWallet = { id: randomId(), name, mnemonic: phrase };
    this.write([...entries, saved]);
    return { id: saved.id, name, entropy: mnemonicToEntropy(phrase, wordlist) };
  }

  forget(id: string): void {
    const entries = this.readForChange();
    this.write(entries.filter((entry) => savedWalletOf(entry)?.id !== id));
  }

  /** Whether a storage event is a change to the saved wallet list. */
  static isChange(event: StorageEvent): boolean {
    return event.key === SAVED_WALLETS_KEY || event.key === null;
  }

  private read(): StoredList {
    const raw = this.storage.getItem(SAVED_WALLETS_KEY);
    if (raw === null) return { readable: true, entries: [] };
    try {
      const parsed: unknown = JSON.parse(raw);
      return Array.isArray(parsed)
        ? { readable: true, entries: parsed }
        : { readable: false };
    } catch {
      return { readable: false };
    }
  }

  /** The stored entries to build a change on, or a refusal that leaves storage as it is. */
  private readForChange(): unknown[] {
    const stored = this.read();
    if (!stored.readable) throw new Error(UNREADABLE);
    return stored.entries;
  }

  private write(entries: unknown[]): void {
    this.storage.setItem(SAVED_WALLETS_KEY, JSON.stringify(entries));
  }
}

/**
 * A random wallet id. `crypto.randomUUID` is only defined in secure contexts,
 * and this host is also opened over plain http on a local network.
 */
function randomId(): string {
  return Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

function normalizeMnemonic(mnemonic: string): string {
  return mnemonic.trim().normalize("NFKD").split(/\s+/).join(" ");
}

/** The wallet a stored entry describes, or null when it is malformed or its phrase is invalid. */
function toWallet(entry: unknown): Wallet | null {
  const saved = savedWalletOf(entry);
  if (saved === null) return null;
  try {
    return {
      id: saved.id,
      name: saved.name,
      entropy: mnemonicToEntropy(saved.mnemonic, wordlist),
    };
  } catch {
    return null;
  }
}

function savedWalletOf(value: unknown): SavedWallet | null {
  if (typeof value !== "object" || value === null) return null;
  const { id, name, mnemonic } = value as Record<string, unknown>;
  return typeof id === "string" &&
    typeof name === "string" &&
    typeof mnemonic === "string"
    ? { id, name, mnemonic }
    : null;
}
