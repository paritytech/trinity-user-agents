import {
  lookupChatIdentity,
  type ChainDirectory,
} from "../chat/nova/directory.js";
import { equalBytes } from "../chat/nova/codec.js";
import type { WalletKeys } from "./wallet-auth.js";

/**
 * The X25519 chat key published for an identity account: the inbox key a
 * request is sealed to, and the key its host proof is checked with. Read from
 * the chain records chat already reads (People `Resources.Consumers` and
 * Asset Hub `DotnsGateway.AccountNames`), never from the peer.
 */
export type PublishedKey =
  /** `name`: the username the same records give the account, or null. */
  | { ok: true; chatKey: Uint8Array; name: string | null }
  /** `unread` when the chain could not be read: asking again may answer. */
  | { ok: false; reason: string; unread: boolean };

export async function publishedChatKey(
  directory: ChainDirectory,
  account: Uint8Array,
): Promise<PublishedKey> {
  const refuse = (reason: string): PublishedKey => ({
    ok: false,
    reason,
    unread: false,
  });
  let identity;
  try {
    identity = await lookupChatIdentity(directory, account);
  } catch (error) {
    return {
      ok: false,
      reason: `its chat identity could not be read: ${String(error)}`,
      unread: true,
    };
  }
  if (identity === null) return refuse("it has no chat identity on chain");
  if (identity.conflict)
    return refuse("Asset Hub and the People chain publish different chat keys");
  if (identity.chatKey === null)
    return refuse("it publishes no X25519 chat key");
  return { ok: true, chatKey: identity.chatKey, name: identity.name };
}

/**
 * Why this wallet cannot prove its user now. `again` when the chain could
 * not be read, so asking later may find no problem.
 */
export interface IdentityProblem {
  reason: string;
  again: boolean;
}

/**
 * Why this wallet cannot prove its user to contacts, or null when it can:
 * the chain must publish this wallet's own chat key for its identity
 * account, or contacts would check the host proof with another key. The host
 * never publishes or registers one itself.
 */
export async function ownIdentityProblem(
  directory: ChainDirectory,
  keys: Pick<WalletKeys, "identity" | "chatPublic">,
): Promise<IdentityProblem | null> {
  const published = await publishedChatKey(directory, keys.identity);
  if (!published.ok)
    return {
      reason: `Your identity account: ${published.reason}.`,
      again: published.unread,
    };
  return equalBytes(published.chatKey, keys.chatPublic)
    ? null
    : {
        reason:
          "The chat key published for your identity account is not this wallet's.",
        again: false,
      };
}
