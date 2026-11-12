import { xchacha20poly1305 } from "@noble/ciphers/chacha.js";
import { hkdf } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import {
  fromHex,
  toHex,
  utf8,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import { FieldError } from "../../bridges/fields/protocol.js";
import { x25519Pair, type X25519Pair } from "../invitations/sealing.js";

/**
 * PROTOTYPE. The one receiving key this host keeps per wallet, network and
 * product, across reloads on this device.
 *
 * The key's X25519 secret is random. It is stored only encrypted
 * (XChaCha20-Poly1305, random 24-byte nonce) under a wrapping key derived
 * from the signed-in wallet's entropy for this purpose alone (HKDF-SHA256,
 * {@link WRAP_DOMAIN}, salted with the network id). The associated data binds
 * the network, product, key id and public key, so a record copied to another
 * slot does not open. Nothing opens while no wallet is signed in:
 * {@link ReceiveKeys.lock} forgets the wrapping key and keeps the records.
 *
 * Storage is this origin's `localStorage`, where this development host also
 * keeps its wallets' recovery phrases in plain text. The encryption keeps
 * secrets apart per wallet and product and out of plain sight; it is no
 * stronger than the wallet store and is not a production key store. Same
 * device only: no backup, rotation, other device or recovery. Clearing the
 * site's storage loses the key, and every envelope sealed to it.
 */

const WRAP_DOMAIN = "truapi-web-host/fields/v1/receive-key-wrap";
const PREFIX = "truapi-web-signing-host:fields:v1";
/** The most product accounts one key is bound to. */
const MAX_ACCOUNTS = 8;

/** The stored record: public parts in clear, the secret sealed. */
interface StoredKey {
  keyId: string;
  publicKey: string;
  created: number;
  /** Product accounts this host bound the key to, `0x` hex. */
  accounts: string[];
  nonce: string;
  sealed: string;
}

/** A receiving key's public parts. */
export interface KeyRecord {
  keyId: string;
  publicKey: string;
  created: number;
  accounts: string[];
}

/** Who the keys belong to: the signed-in wallet on a network. */
export interface KeyOwner {
  network: string;
  walletId: string;
  entropy: Uint8Array;
}

const associated = (
  network: string,
  product: string,
  keyId: string,
  publicKey: string,
): Uint8Array =>
  utf8(`web-fields/receive-key/v1|${network}|${product}|${keyId}|${publicKey}`);

const isHexOf = (value: unknown, bytes: number): value is string =>
  typeof value === "string" &&
  new RegExp(`^0x[0-9a-f]{${bytes * 2}}$`).test(value);

function readStored(raw: string | null): StoredKey | null {
  if (raw === null) return null;
  try {
    const item = JSON.parse(raw) as Record<string, unknown>;
    return isHexOf(item.keyId, 16) &&
      isHexOf(item.publicKey, 32) &&
      isHexOf(item.nonce, 24) &&
      typeof item.sealed === "string" &&
      typeof item.created === "number" &&
      Array.isArray(item.accounts) &&
      item.accounts.length <= MAX_ACCOUNTS &&
      item.accounts.every((account) => isHexOf(account, 32))
      ? (item as unknown as StoredKey)
      : null;
  } catch {
    return null;
  }
}

export class ReceiveKeys {
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
      throw new FieldError(
        "session",
        "Sign in with a wallet to use receiving keys.",
      );
    return this.owner;
  }

  private slot(product: string): string {
    const owner = this.current();
    return `${PREFIX}:${owner.network}:${owner.walletId}:${product}`;
  }

  private read(product: string): StoredKey | null {
    return readStored(this.storage.getItem(this.slot(product)));
  }

  /** `product`'s key, made and stored now if it has none. */
  ensure(product: string): KeyRecord {
    const kept = this.read(product);
    if (kept !== null) return publicPart(kept);
    const owner = this.current();
    const secret = this.random(32);
    const keyId = toHex(this.random(16));
    const publicKey = toHex(x25519Pair(secret).public);
    const nonce = this.random(24);
    const sealed = xchacha20poly1305(
      owner.wrap,
      nonce,
      associated(owner.network, product, keyId, publicKey),
    ).encrypt(secret);
    secret.fill(0);
    const stored: StoredKey = {
      keyId,
      publicKey,
      created: this.now(),
      accounts: [],
      nonce: toHex(nonce),
      sealed: toHex(sealed),
    };
    this.storage.setItem(this.slot(product), JSON.stringify(stored));
    return publicPart(stored);
  }

  /** Record that this host bound `product`'s key to `account`. */
  bound(product: string, account: string): void {
    const kept = this.read(product);
    if (kept === null) throw new FieldError("unavailable", "No receiving key.");
    const lower = account.toLowerCase();
    if (kept.accounts.includes(lower)) return;
    if (kept.accounts.length >= MAX_ACCOUNTS)
      throw new FieldError(
        "limit",
        "This app's receiving key is bound to too many accounts.",
      );
    kept.accounts.push(lower);
    this.storage.setItem(this.slot(product), JSON.stringify(kept));
  }

  /**
   * `product`'s key with its pair when its id is `keyId`, or null. Throws
   * when the stored record does not open: changed, moved or another
   * wallet's. The caller zeroes the pair's secret.
   */
  open(
    product: string,
    keyId: string,
  ): (KeyRecord & { pair: X25519Pair }) | null {
    const owner = this.current();
    const kept = this.read(product);
    if (kept === null || kept.keyId !== keyId.toLowerCase()) return null;
    let secret: Uint8Array;
    try {
      secret = xchacha20poly1305(
        owner.wrap,
        fromHex(kept.nonce),
        associated(owner.network, product, kept.keyId, kept.publicKey),
      ).decrypt(fromHex(kept.sealed));
    } catch {
      throw new FieldError(
        "invalid",
        "The stored receiving key does not open.",
      );
    }
    const pair = x25519Pair(secret);
    if (toHex(pair.public) !== kept.publicKey) {
      secret.fill(0);
      throw new FieldError(
        "invalid",
        "The stored receiving key does not open.",
      );
    }
    return { ...publicPart(kept), pair };
  }
}

const publicPart = ({
  keyId,
  publicKey,
  created,
  accounts,
}: StoredKey): KeyRecord => ({
  keyId,
  publicKey,
  created,
  accounts: [...accounts],
});
