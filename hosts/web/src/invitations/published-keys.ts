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
  | { ok: true; chatKey: Uint8Array }
  | { ok: false; reason: string };

export async function publishedChatKey(
  directory: ChainDirectory,
  account: Uint8Array,
): Promise<PublishedKey> {
  const refuse = (reason: string): PublishedKey => ({ ok: false, reason });
  let identity;
  try {
    identity = await lookupChatIdentity(directory, account);
  } catch (error) {
    return refuse(`its chat identity could not be read: ${String(error)}`);
  }
  if (identity === null) return refuse("it has no chat identity on chain");
  if (identity.conflict)
    return refuse("Asset Hub and the People chain publish different chat keys");
  if (identity.chatKey === null)
    return refuse("it publishes no X25519 chat key");
  return { ok: true, chatKey: identity.chatKey };
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
): Promise<string | null> {
  const published = await publishedChatKey(directory, keys.identity);
  if (!published.ok) return `Your identity account: ${published.reason}.`;
  return equalBytes(published.chatKey, keys.chatPublic)
    ? null
    : "The chat key published for your identity account is not this wallet's.";
}
