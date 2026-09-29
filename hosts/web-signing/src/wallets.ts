import { mnemonicToEntropy, validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  DEV_ACCOUNTS,
  DEV_ACCOUNT_NAMES,
} from "@parity/truapi-host/testing/dev-accounts";

/** A wallet this host can activate a session from. */
export interface Wallet {
  /** Stable id; also the namespace this wallet's storage lives under. */
  id: string;
  /** Label shown in the wallet picker. */
  name: string;
  /** BIP-39 entropy the core derives every key from. */
  entropy: Uint8Array;
}

/** A wallet the user created or imported, as it is persisted. */
interface SavedWallet {
  id: string;
  name: string;
  mnemonic: string;
}

const SAVED_WALLETS_KEY = "truapi-web-signing-host:wallets:v1";
const DEV_WALLET_PREFIX = "dev:";

/**
 * The built-in dev accounts from the test host, so a session can start with
 * no recovery phrase at all. They share their entropy with every other
 * checkout, so anything they sign is public.
 */
export function devWallets(): Wallet[] {
  return DEV_ACCOUNT_NAMES.map((name) => ({
    id: `${DEV_WALLET_PREFIX}${name}`,
    name,
    entropy: DEV_ACCOUNTS[name],
  }));
}

/**
 * Wallets created or imported in this browser.
 *
 * Recovery phrases are stored in plain text in `storage`, which is this
 * origin's `localStorage`. That is the reason this host is for development
 * wallets only: any script on this origin can read them.
 */
export class WalletStore {
  constructor(private readonly storage: Storage) {}

  /** Built-in dev wallets first, then saved ones in the order they were added. */
  list(): Wallet[] {
    return [...devWallets(), ...this.saved().map(toWallet)];
  }

  find(id: string): Wallet | undefined {
    return this.list().find((wallet) => wallet.id === id);
  }

  /** The recovery phrase of a saved wallet, for the user to back it up. */
  mnemonicOf(id: string): string | undefined {
    return this.saved().find((wallet) => wallet.id === id)?.mnemonic;
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
    const saved = this.saved();
    const existing = saved.find((wallet) => wallet.mnemonic === phrase);
    if (existing) return toWallet(existing);
    const wallet: SavedWallet = {
      id: crypto.randomUUID(),
      name,
      mnemonic: phrase,
    };
    this.write([...saved, wallet]);
    return toWallet(wallet);
  }

  forget(id: string): void {
    this.write(this.saved().filter((wallet) => wallet.id !== id));
  }

  /** Whether a storage event is a change to the saved wallet list. */
  static isChange(event: StorageEvent): boolean {
    return event.key === SAVED_WALLETS_KEY || event.key === null;
  }

  private saved(): SavedWallet[] {
    const raw = this.storage.getItem(SAVED_WALLETS_KEY);
    if (raw === null) return [];
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isSavedWallet) : [];
  }

  private write(wallets: SavedWallet[]): void {
    this.storage.setItem(SAVED_WALLETS_KEY, JSON.stringify(wallets));
  }
}

function normalizeMnemonic(mnemonic: string): string {
  return mnemonic.trim().normalize("NFKD").split(/\s+/).join(" ");
}

function toWallet(saved: SavedWallet): Wallet {
  return {
    id: saved.id,
    name: saved.name,
    entropy: mnemonicToEntropy(saved.mnemonic, wordlist),
  };
}

function isSavedWallet(value: unknown): value is SavedWallet {
  if (typeof value !== "object" || value === null) return false;
  const { id, name, mnemonic } = value as Record<string, unknown>;
  return (
    typeof id === "string" &&
    typeof name === "string" &&
    typeof mnemonic === "string"
  );
}
