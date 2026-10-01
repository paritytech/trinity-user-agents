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
}

const KEY_PREFIX = "truapi-web-signing-host:wallet-public:v1:";

function isInfo(value: unknown): value is WalletPublicInfo {
  if (typeof value !== "object" || value === null) return false;
  const { publicKey, username } = value as Record<string, unknown>;
  return (
    typeof publicKey === "string" &&
    (username === undefined || typeof username === "string")
  );
}

/** The public facts kept per wallet id, in this browser. */
export class WalletPublicInfoStore {
  constructor(
    private readonly storage: Pick<
      Storage,
      "getItem" | "setItem" | "removeItem"
    >,
  ) {}

  get(walletId: string): WalletPublicInfo | undefined {
    try {
      const value: unknown = JSON.parse(
        this.storage.getItem(KEY_PREFIX + walletId) ?? "null",
      );
      return isInfo(value) ? value : undefined;
    } catch {
      return undefined;
    }
  }

  /**
   * Keep what is known. A new key replaces an old one with its username dropped,
   * because the name belonged to the old key. A name is kept when the key is the same.
   */
  update(
    walletId: string,
    next: { publicKey: string; username?: string },
  ): void {
    const known = this.get(walletId);
    const username =
      next.username ??
      (known?.publicKey === next.publicKey ? known.username : undefined);
    try {
      this.storage.setItem(
        KEY_PREFIX + walletId,
        JSON.stringify(
          username === undefined
            ? { publicKey: next.publicKey }
            : { publicKey: next.publicKey, username },
        ),
      );
    } catch {
      // The label falls back to what is known; nothing else depends on this.
    }
  }

  forget(walletId: string): void {
    this.storage.removeItem(KEY_PREFIX + walletId);
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
