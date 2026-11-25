import { type HexString } from '@/shared/types';

/** How much the People chain vouches for an account's username. */
export type Credibility = { type: 'Lite' } | { type: 'Person'; alias: HexString; lastUpdate: string | null };

/**
 * An account's public identity as the People chain records it.
 *
 * `identifierKey` is the account's X25519 chat key, unwrapped from its RFC-0004
 * container and hex-encoded. `null` when the record carries a keypair type this
 * host cannot encrypt to — a normal condition for an account that has not enabled
 * chat, not a fault.
 */
export type ConsumerIdentity = {
  accountId: string;
  fullUsername: string | null;
  liteUsername: string;
  credibility: Credibility;
  identifierKey: HexString | null;
};
