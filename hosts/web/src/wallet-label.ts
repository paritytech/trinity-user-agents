import { DEFAULT_NETWORK, normalizeNetwork } from "./network-scope.js";

/** The default name an imported wallet gets when none is typed. */
const GENERIC_NAME = "Imported wallet";

/**
 * Public facts about a saved wallet, learned while it is signed in and kept so
 * the picker can tell wallets apart while signed out. None of it is secret,
 * and none of it is read from a wallet that was never signed in.
 */
export interface WalletPublicInfo {
  /** The root public key the session reported, as hex. */
  publicKey: string;
  /** A username from the People chain record or the session, when one was found. */
  username?: string;
  /**
   * The lite username from the People chain record that the wallet's session
   * was last given. Sign-in starts the session with it, before the chain is
   * read again.
   */
  sessionUsername?: string;
}

const KEY_PREFIX = "truapi-web-signing-host:wallet-public:v1:";

/**
 * The storage key one network keeps a wallet's public facts under. A network
 * other than the default gets a `network:<id>:` segment; the default keeps the
 * original key. The root public key is the same on every network, but the
 * record is per network so a username learned on one network never shows on
 * another.
 */
function keyFor(walletId: string, network: string): string {
  const id = normalizeNetwork(network);
  return id === DEFAULT_NETWORK
    ? KEY_PREFIX + walletId
    : `truapi-web-signing-host:network:${id}:wallet-public:v1:${walletId}`;
}

function isInfo(value: unknown): value is WalletPublicInfo {
  if (typeof value !== "object" || value === null) return false;
  const { publicKey, username, sessionUsername } = value as Record<
    string,
    unknown
  >;
  return (
    typeof publicKey === "string" &&
    (username === undefined || typeof username === "string") &&
    (sessionUsername === undefined || typeof sessionUsername === "string")
  );
}

/** The public facts kept per wallet and network, in this browser. */
export class WalletPublicInfoStore {
  constructor(
    private readonly storage: Pick<
      Storage,
      "getItem" | "setItem" | "removeItem" | "length" | "key"
    >,
  ) {}

  get(
    walletId: string,
    network: string = DEFAULT_NETWORK,
  ): WalletPublicInfo | undefined {
    try {
      const value: unknown = JSON.parse(
        this.storage.getItem(keyFor(walletId, network)) ?? "null",
      );
      return isInfo(value) ? value : undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Keep what is known for this wallet on this network. A new key replaces an
   * old one with its names dropped, because the names belonged to the old
   * key. A name is kept when the key is the same and `next` does not give
   * one. A `sessionUsername` of null drops the stored one.
   */
  update(
    walletId: string,
    next: {
      publicKey: string;
      username?: string;
      sessionUsername?: string | null;
    },
    network: string = DEFAULT_NETWORK,
  ): void {
    const known = this.get(walletId, network);
    const sameKey = known?.publicKey === next.publicKey;
    const username = next.username ?? (sameKey ? known.username : undefined);
    const sessionUsername =
      next.sessionUsername === null
        ? undefined
        : (next.sessionUsername ??
          (sameKey ? known.sessionUsername : undefined));
    const info: WalletPublicInfo = { publicKey: next.publicKey };
    if (username !== undefined) info.username = username;
    if (sessionUsername !== undefined) info.sessionUsername = sessionUsername;
    try {
      this.storage.setItem(keyFor(walletId, network), JSON.stringify(info));
    } catch {
      // The label falls back to what is known; nothing else depends on this.
    }
  }

  /**
   * Drop a wallet's public facts on every network, as when the wallet is
   * forgotten. The facts are public, but a forgotten wallet should leave none.
   */
  forget(walletId: string): void {
    const suffix = `wallet-public:v1:${walletId}`;
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

/** `0x625e…b715`: enough to tell keys apart in a list. */
export function shortKey(hex: string): string {
  return hex.length > 14 ? `${hex.slice(0, 6)}…${hex.slice(-4)}` : hex;
}

/**
 * The picker label for one wallet: the name typed at import, the username when
 * one was found, and a short public key. A wallet never signed in has no key
 * to show, so it gets a short tag from its own id, which is public and only
 * tells wallets apart.
 */
export function walletLabel(
  wallet: { id: string; name: string },
  info: WalletPublicInfo | undefined,
): string {
  const name = wallet.name.trim();
  const parts = [
    ...(name !== "" && name !== GENERIC_NAME ? [name] : []),
    ...(info?.username !== undefined && info.username !== name
      ? [info.username]
      : []),
  ];
  const tail =
    info === undefined ? `#${wallet.id.slice(0, 4)}` : shortKey(info.publicKey);
  return parts.length === 0
    ? `${GENERIC_NAME} · ${tail}`
    : `${parts.join(" · ")} · ${tail}`;
}

/** The username a People chain reading holds, identity account first. */
export function usernameFrom(
  readings: {
    role: string;
    consumer: { fullUsername: string | null; liteUsername: string } | null;
  }[],
): string | undefined {
  for (const role of ["identity", "root"]) {
    const consumer = readings.find((item) => item.role === role)?.consumer;
    if (consumer) return consumer.fullUsername ?? consumer.liteUsername;
  }
  return undefined;
}

/**
 * The lite username a local session should carry, from a People chain reading:
 * the identity account's record first, then the root key's. Null when neither
 * account has a record. Undefined when the record that would decide it could
 * not be decoded, so the reading says nothing about the name.
 */
export function sessionUsernameFrom(
  readings: {
    role: string;
    consumer: { liteUsername: string } | null;
    problem?: string;
  }[],
): string | null | undefined {
  for (const role of ["identity", "root"]) {
    const reading = readings.find((item) => item.role === role);
    if (reading === undefined) continue;
    if (reading.problem !== undefined) return undefined;
    if (reading.consumer) return reading.consumer.liteUsername;
  }
  return null;
}
