import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import {
  fromHex,
  toHex,
  utf8,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import { LocketError } from "../../bridges/locket/protocol.js";
import { x25519Pair, type X25519Pair } from "../invitations/sealing.js";

/**
 * PROTOTYPE. The recipient keys this host keeps, across reloads on this
 * device: one per wallet, network, product and product account, so two
 * accounts of one product never publish the same key.
 *
 * Each key's X25519 secret is random. It is stored only encrypted
 * (XChaCha20-Poly1305, random 24-byte nonce) under a wrapping key derived
 * from the signed-in wallet's entropy for this purpose alone (HKDF-SHA256,
 * {@link WRAP_DOMAIN}, salted with the network id). The associated data binds
 * the network, product, account, key id and public key, so a record copied
 * to another slot or account does not open. Nothing opens while no wallet is
 * signed in: {@link RecipientKeys.lock} forgets the wrapping key and keeps the
 * records.
 *
 * Storage is this origin's `localStorage`, where this development host also
 * keeps its wallets' recovery phrases in plain text. The encryption keeps
 * secrets apart per wallet and product and out of plain sight; it is no
 * stronger than the wallet store and is not a production key store. Same
 * device only: no backup, rotation, other device or recovery. Clearing the
 * site's storage loses the keys, and every envelope sealed to them.
 *
 * Earlier builds kept one key per product, shared by its accounts. Such a
 * record is still read, only to open envelopes already sealed to it
 * ({@link RecipientKeys.open}); no new binding is ever made for it.
 */

// These strings derive and name stored records: changing one makes every
// stored key unreadable.
const WRAP_DOMAIN = "truapi-web-host/fields/v1/receive-key-wrap";
const PREFIX = "truapi-web-signing-host:fields:v1";
/** The most accounts of one product that keep a recipient key. */
export const MAX_ACCOUNTS = 16;

/** A recipient key as stored: public parts in clear, the secret sealed. */
interface StoredKey {
  keyId: string;
  publicKey: string;
  created: number;
  nonce: string;
  sealed: string;
}

/** A per-account key: the 32-byte account index that asked for it, and the account the core bound it to. */
interface AccountKey extends StoredKey {
  index: string;
  account: string;
}

/** The per-product record of earlier builds: one key, the accounts bound to it. */
interface LegacyKey extends StoredKey {
  accounts: string[];
}

/** A recipient key's public parts. */
export interface KeyRecord {
  keyId: string;
  publicKey: string;
  created: number;
}

/** A key that opens envelopes, and the accounts an envelope sealed to it may name. */
export interface OpenKey extends KeyRecord {
  pair: X25519Pair;
  /** One account, or for a legacy shared key every account it was bound to. */
  accounts: string[];
  legacy: boolean;
}

/** Who the keys belong to: the signed-in wallet on a network. */
export interface KeyOwner {
  network: string;
  walletId: string;
  entropy: Uint8Array;
}

/**
 * What a sealed secret is bound to. A legacy key has no account in it, as
 * the earlier builds sealed it.
 */
const associated = (
  network: string,
  product: string,
  key: { keyId: string; publicKey: string; account?: string },
): Uint8Array =>
  utf8(
    key.account === undefined
      ? `web-fields/receive-key/v1|${network}|${product}|${key.keyId}|${key.publicKey}`
      : `web-fields/receive-key/v2|${network}|${product}|${key.account}|${key.keyId}|${key.publicKey}`,
  );

const isHexOf = (value: unknown, bytes: number): value is string =>
  typeof value === "string" &&
  new RegExp(`^0x[0-9a-f]{${bytes * 2}}$`).test(value);

const isStored = (item: Record<string, unknown>): boolean =>
  isHexOf(item.keyId, 16) &&
  isHexOf(item.publicKey, 32) &&
  isHexOf(item.nonce, 24) &&
  typeof item.sealed === "string" &&
  typeof item.created === "number";

const parse = (raw: string | null): unknown => {
  if (raw === null) return null;
  try {
    return JSON.parse(raw);
  } catch {
    return null;
  }
};

const isAccountKey = (item: unknown): item is AccountKey =>
  typeof item === "object" &&
  item !== null &&
  isStored(item as Record<string, unknown>) &&
  isHexOf((item as Record<string, unknown>).index, 32) &&
  isHexOf((item as Record<string, unknown>).account, 32);

/**
 * The per-account keys a slot holds, and whether any of it is unreadable.
 * An unreadable slot or entry is kept as it is, never dropped or rewritten.
 */
function readAccountKeys(raw: string | null): {
  keys: AccountKey[];
  damaged: boolean;
} {
  if (raw === null) return { keys: [], damaged: false };
  const list = parse(raw);
  if (!Array.isArray(list)) return { keys: [], damaged: true };
  const keys = list.filter(isAccountKey);
  return { keys, damaged: keys.length !== list.length };
}

/** The legacy key a slot holds, null when there is none, or `"damaged"`. */
function readLegacy(raw: string | null): LegacyKey | null | "damaged" {
  if (raw === null) return null;
  const item = parse(raw) as Record<string, unknown> | null;
  return typeof item === "object" &&
    item !== null &&
    isStored(item) &&
    Array.isArray(item.accounts) &&
    item.accounts.every((account) => isHexOf(account, 32))
    ? (item as unknown as LegacyKey)
    : "damaged";
}

/**
 * A stored key that does not decrypt: changed, moved, or sealed under other
 * wallet entropy. It is this device's storage, not the envelope, so it is
 * not reported as a bad envelope.
 */
const unreadable = () =>
  new LocketError(
    "unavailable",
    "This device's recipient key for it cannot be read.",
  );

const publicPart = ({ keyId, publicKey, created }: StoredKey): KeyRecord => ({
  keyId,
  publicKey,
  created,
});

/** A key not yet bound: made by {@link RecipientKeys.prepare}, kept by {@link RecipientKeys.keep}. */
export interface PreparedKey extends KeyRecord {
  secret: Uint8Array;
}

export class RecipientKeys {
  private owner: {
    network: string;
    walletId: string;
    wrap: Uint8Array;
  } | null = null;

  constructor(
    private readonly storage: Pick<Storage, "getItem" | "setItem">,
    private readonly random: RandomBytes,
    private readonly now: () => number = Date.now,
  ) {}

  /** Make `owner`'s keys usable, until {@link lock}. */
  unlock(owner: KeyOwner): void {
    this.lock();
    this.owner = {
      network: owner.network,
      walletId: owner.walletId,
      wrap: hkdf(
        sha256,
        owner.entropy,
        utf8(owner.network),
        utf8(WRAP_DOMAIN),
        32,
      ),
    };
  }

  /** Forget the wrapping key. Stored keys stay and open again after {@link unlock}. */
  lock(): void {
    this.owner?.wrap.fill(0);
    this.owner = null;
  }

  private current() {
    if (this.owner === null)
      throw new LocketError(
        "session",
        "Sign in with a wallet to use recipient keys.",
      );
    return this.owner;
  }

  /** The earlier builds' slot: one key per product. */
  private legacySlot(product: string): string {
    const owner = this.current();
    return `${PREFIX}:${owner.network}:${owner.walletId}:${product}`;
  }

  /** The slot of a product's per-account keys. */
  private slot(product: string): string {
    return `${this.legacySlot(product)}:accounts`;
  }

  private accountKeys(product: string) {
    return readAccountKeys(this.storage.getItem(this.slot(product)));
  }

  /**
   * The key `product` keeps for its account at `index`, or null when it
   * has none yet. The account it names is the one the core bound it to.
   */
  find(
    product: string,
    index: string,
  ): (KeyRecord & { account: string }) | null {
    const { keys, damaged } = this.accountKeys(product);
    const found = keys.find((key) => key.index === index.toLowerCase());
    // A damaged entry may be this account's key: do not report none.
    if (found === undefined && damaged) throw unreadable();
    return found === undefined
      ? null
      : { ...publicPart(found), account: found.account };
  }

  /** A fresh random key, held in memory until {@link keep} stores it for an account. */
  prepare(): PreparedKey {
    this.current();
    const secret = this.random(32);
    return {
      keyId: toHex(this.random(16)),
      publicKey: toHex(x25519Pair(secret).public),
      created: this.now(),
      secret,
    };
  }

  /**
   * Store `key` as `product`'s key for the account the core resolved
   * `index` to, and zero its secret. Refuses an index or account that
   * already has a key, and more than {@link MAX_ACCOUNTS} accounts.
   */
  keep(
    product: string,
    index: string,
    account: string,
    key: PreparedKey,
  ): KeyRecord {
    const owner = this.current();
    try {
      const { keys, damaged } = this.accountKeys(product);
      // Writing would drop the damaged entries, and with them their orders.
      if (damaged) throw unreadable();
      const lowerIndex = index.toLowerCase();
      const lowerAccount = account.toLowerCase();
      if (
        keys.some(
          (kept) => kept.index === lowerIndex || kept.account === lowerAccount,
        )
      )
        throw new LocketError(
          "busy",
          "This account already has a recipient key.",
        );
      if (keys.length >= MAX_ACCOUNTS)
        throw new LocketError(
          "limit",
          "This app keeps recipient keys for too many accounts.",
        );
      const nonce = this.random(24);
      const record: AccountKey = {
        keyId: key.keyId,
        publicKey: key.publicKey,
        created: key.created,
        index: lowerIndex,
        account: lowerAccount,
        nonce: toHex(nonce),
        sealed: toHex(
          xchacha20poly1305(
            owner.wrap,
            nonce,
            associated(owner.network, product, {
              keyId: key.keyId,
              publicKey: key.publicKey,
              account: lowerAccount,
            }),
          ).encrypt(key.secret),
        ),
      };
      this.storage.setItem(
        this.slot(product),
        JSON.stringify([...keys, record]),
      );
      return publicPart(record);
    } finally {
      key.secret.fill(0);
    }
  }

  /**
   * `product`'s key with id `keyId`, opened, or null when it has none: one
   * of its accounts' keys, or else the legacy shared key, read only so
   * envelopes sealed to it still open. Throws `unavailable` when the stored record does
   * not open: changed, moved or another wallet's. The caller zeroes the
   * pair's secret.
   */
  open(product: string, keyId: string): OpenKey | null {
    const owner = this.current();
    const id = keyId.toLowerCase();
    const accountKeys = this.accountKeys(product);
    const perAccount = accountKeys.keys.find((key) => key.keyId === id);
    const stored =
      perAccount === undefined
        ? readLegacy(this.storage.getItem(this.legacySlot(product)))
        : null;
    const legacy = stored === "damaged" ? null : stored;
    const found = perAccount ?? (legacy?.keyId === id ? legacy : undefined);
    if (found === undefined) {
      // An unreadable record may be the key asked for: do not report none.
      if (accountKeys.damaged || stored === "damaged") throw unreadable();
      return null;
    }
    let secret: Uint8Array;
    try {
      secret = xchacha20poly1305(
        owner.wrap,
        fromHex(found.nonce),
        associated(owner.network, product, {
          keyId: found.keyId,
          publicKey: found.publicKey,
          ...(perAccount === undefined ? {} : { account: perAccount.account }),
        }),
      ).decrypt(fromHex(found.sealed));
    } catch {
      throw unreadable();
    }
    const pair = x25519Pair(secret);
    if (toHex(pair.public) !== found.publicKey) {
      secret.fill(0);
      throw unreadable();
    }
    return {
      ...publicPart(found),
      pair,
      accounts:
        perAccount === undefined ? [...legacy!.accounts] : [perAccount.account],
      legacy: perAccount === undefined,
    };
  }
}
