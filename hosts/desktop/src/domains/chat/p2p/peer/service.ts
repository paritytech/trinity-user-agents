/**
 * Pure helpers for peer search: filtering its results, and the proof-of-compute work function the
 * identity backend's search route can demand.
 */

import { sha256 } from '@noble/hashes/sha2.js';
import { AccountId } from '@polkadot-api/substrate-bindings';

import { p2pService } from '../service';
import { type SearchResult } from '../types';

// Decode any SS58 address to its 32-byte public key. The codec validates the
// checksum and length but ignores the network prefix on decode, so the match
// holds regardless of which SS58 format the backend minted `accountId`
// in.
const ss58ToPublicKey = AccountId().enc;

/**
 * Remove the current user's own account from contact search results.
 *
 * A username search resolves against the user's **identity** account (the
 * account a username is registered under on-chain), so a search for the current
 * user's own username returns their identity account. Self-chats are not a
 * supported flow, so that entry is dropped here.
 *
 * `selfIdentityPublicKey` is the raw 32-byte sr25519 identity public key
 * (`userIdentity.identitySr25519PublicKey`) — NOT the per-device statement
 * account. The username is registered under the identity account, so comparing
 * against the statement account never matches and self leaks into results.
 * Comparison is on decoded public-key bytes, so it is independent of the SS58
 * prefix.
 */
function excludeSelfFromSearchResults(results: SearchResult[], selfIdentityPublicKey: Uint8Array): SearchResult[] {
  return results.filter(result => !p2pService.bytesEqual(ss58ToPublicKey(result.accountId), selfIdentityPublicKey));
}

/**
 * The 32-byte work preimage: the session UUID's raw bytes, then the timestamp and counter as
 * big-endian u64s.
 *
 * Taken from the backend's own `crates/username-indexer/src/poc/solution.rs`, NOT from its public
 * docs — those render it as `sha256(sessionId || timestamp || counter)`, which reads as string
 * concatenation and is not what the server computes.
 */
function workPreimage(sessionId: string, timestampMs: number, counter: number): Uint8Array {
  const preimage = new Uint8Array(32);
  const hex = sessionId.replaceAll('-', '');
  for (let i = 0; i < 16; i++) {
    preimage[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }

  const view = new DataView(preimage.buffer);
  view.setBigUint64(16, BigInt(timestampMs), false);
  view.setBigUint64(24, BigInt(counter), false);

  return preimage;
}

/**
 * Leading zero bits of the work digest, counted over its first big-endian u32 only.
 *
 * Counting over the whole 32-byte digest would accept work the server rejects. The server counts
 * one u32 (`Math.clz32` in the implementation this was ported from), which is also why difficulty
 * is capped at 32.
 */
function leadingZeroBits(sessionId: string, timestampMs: number, counter: number): number {
  const digest = sha256(workPreimage(sessionId, timestampMs, counter));

  // Read the bytes arithmetically: a `Uint8Array` can be a view into a larger buffer, so
  // `new DataView(digest.buffer)` would not necessarily start at the digest.
  return Math.clz32(((digest[0] ?? 0) << 24) | ((digest[1] ?? 0) << 16) | ((digest[2] ?? 0) << 8) | (digest[3] ?? 0));
}

/** A solved puzzle, in the backend's vocabulary. */
type Solution = {
  sessionId: string;
  timestampMs: number;
  difficulty: number;
  counter: number;
  checksum: string;
};

/**
 * The `Proof-Of-Compute` header value.
 *
 * Standard padded base64 — `btoa`. The server decodes with the standard alphabet, so base64url
 * would be rejected as malformed.
 */
function encodeProofOfComputeHeader({ sessionId, timestampMs, difficulty, counter, checksum }: Solution): string {
  return btoa(`${sessionId}:${timestampMs}:${difficulty}:${counter}:${checksum}`);
}

export const peerSearchService = {
  excludeSelfFromSearchResults,
  leadingZeroBits,
  encodeProofOfComputeHeader,
};
