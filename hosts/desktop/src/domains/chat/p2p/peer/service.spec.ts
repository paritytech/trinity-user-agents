import { AccountId } from '@polkadot-api/substrate-bindings';
import { describe, expect, it } from 'vitest';

import { type SearchResult } from '../types';

import { peerSearchService } from './service';

const result = (accountId: string, username: string): SearchResult => ({
  accountId,
  username,
  status: 'ASSIGNED',
});

const SELF_KEY = new Uint8Array(32).fill(1);
const PEER_KEY = new Uint8Array(32).fill(2);

// Encode a public key to SS58 at a given network prefix (42 = generic substrate).
const ss58 = (publicKey: Uint8Array, prefix = 42): string => AccountId(prefix).dec(publicKey);

describe('peerSearchService.excludeSelfFromSearchResults', () => {
  it('drops the entry whose account matches the current user identity', () => {
    const results = [result(ss58(SELF_KEY), 'me'), result(ss58(PEER_KEY), 'alice')];

    const filtered = peerSearchService.excludeSelfFromSearchResults(results, SELF_KEY);

    expect(filtered).toEqual([result(ss58(PEER_KEY), 'alice')]);
  });

  it('keeps every entry when none matches the current user', () => {
    const results = [result(ss58(PEER_KEY), 'alice'), result(ss58(new Uint8Array(32).fill(3)), 'bob')];

    expect(peerSearchService.excludeSelfFromSearchResults(results, SELF_KEY)).toEqual(results);
  });

  it('drops all entries when only self matches', () => {
    const results = [result(ss58(SELF_KEY), 'me')];

    expect(peerSearchService.excludeSelfFromSearchResults(results, SELF_KEY)).toEqual([]);
  });

  it('matches self regardless of the SS58 network prefix the backend used', () => {
    // Same identity key, encoded under a different network prefix (2 = Kusama).
    const results = [result(ss58(SELF_KEY, 2), 'me'), result(ss58(PEER_KEY), 'alice')];

    const filtered = peerSearchService.excludeSelfFromSearchResults(results, SELF_KEY);

    expect(filtered).toEqual([result(ss58(PEER_KEY), 'alice')]);
  });
});

// The vectors are the backend's own, from `crates/username-indexer/src/poc/solution.rs` in
// `paritytech/device-uniqueness-backend-community`. They are the only proof this port matches the server:
// the public API docs render the preimage as `sha256(sessionId || timestamp || counter)`, which
// reads as string concatenation and is not what is computed.
describe('peerSearchService.leadingZeroBits', () => {
  const SESSION = '1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed';
  const TIMESTAMP = 1_700_000_000_000;

  it.each([
    [0, 3],
    [12_345, 0],
  ])('matches the backend work vector at counter %i', (counter, expected) => {
    expect(peerSearchService.leadingZeroBits(SESSION, TIMESTAMP, counter)).toBe(expected);
  });
});

describe('peerSearchService.encodeProofOfComputeHeader', () => {
  const CHECKSUM = 'c8828951fd6c123fdbf6501f111d27dd3f260839344a7370e0dd8f20e2c40482';

  it('encodes the five colon-joined fields as standard padded base64', () => {
    const header = peerSearchService.encodeProofOfComputeHeader({
      sessionId: '1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed',
      timestampMs: 1_700_000_000_000,
      difficulty: 16,
      counter: 16_763,
      checksum: CHECKSUM,
    });

    expect(atob(header).split(':')).toEqual(['1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed', '1700000000000', '16', '16763', CHECKSUM]);
    // Padded, standard alphabet: the server decodes with STANDARD and rejects base64url as
    // malformed, which it answers 400 for.
    expect(header.length % 4).toBe(0);
  });
});
