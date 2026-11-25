/**
 * Peer resolution for P2P chat — username search and chat-key lookup.
 *
 * Search is a plain call to the identity backend; the chat key comes off the
 * account's People-chain record, which `@/domains/network` owns and caches. The
 * host used to get both through host-chat's `AccountService`, which typed its
 * identity source against host-papp — this reads the chain directly instead.
 */

import { fromHex } from 'polkadot-api/utils';
import * as v from 'valibot';

import { consumerIdentityUseCase } from '@/domains/network';
import { type SearchResult } from '../types';

import { type Puzzle, puzzleSchema, searchResponseSchema } from './schemas';
import { peerSearchService } from './service';

// The backend defaults to 100 and clamps to 1,000. 50 is ours because the page size is also the
// render size: `SearchResultsPanel` maps every returned row into a scroll container with no
// virtualization, and a type-ahead user refines the query rather than scrolling to row 87.
const SEARCH_PAGE_SIZE = 50;

// The puzzle route is absent on deployments with proof of compute disabled, where it 404s fast.
// This bounds the case where it neither answers nor refuses.
const POC_ISSUE_TIMEOUT_MS = 500;

// Difficulty 16 — what deployments configure — is ~65k hashes, well under 100ms. The budget is for
// a deployment that raises it: the protocol allows 32, which no interactive search can afford.
const MINE_BUDGET_MS = 1_500;

// Counters between yields. ~4ms of uninterrupted hashing, a quarter of a frame.
const MINE_CHUNK = 4_096;

async function searchUsernames(
  endpoint: string,
  query: string,
  proofOfCompute: string | null,
  onProofOfComputeRequired: VoidFunction,
): Promise<SearchResult[]> {
  const params = new URLSearchParams({ prefix: query, limit: String(SEARCH_PAGE_SIZE) });
  const response = await fetch(`${endpoint}usernames/search?${params.toString()}`, {
    method: 'GET',
    headers: proofOfCompute ? { Accept: 'application/json', 'Proof-Of-Compute': proofOfCompute } : { Accept: 'application/json' },
  });

  if (!response.ok) {
    // These reach the user verbatim: `useContactSearch` puts the message into its error state and
    // the search panel renders it.
    if (response.status === 429) {
      throw new Error('Too many searches. Try again in a moment.');
    }
    if (response.status === 402) {
      // This status is the server saying it DOES want a puzzle — which is the one piece of
      // evidence that can undo a probe disabled by a transient failure.
      onProofOfComputeRequired();
      // The user cannot act on "proof of compute", so the message does not name it.
      console.warn('[peer-search] search refused for want of proof of compute', { endpoint });
      throw new Error('Search is temporarily unavailable.');
    }

    throw new Error(`status: ${response.status}, ${await response.text()}`);
  }

  return v.parse(searchResponseSchema, await response.json()).usernames;
}

/**
 * Ask for a puzzle, or `null` when this deployment offers none.
 *
 * Never throws. A 404 here is the normal case — proof of compute is off by default, and the route
 * is then not mounted — and search must keep working anonymously when it is.
 */
async function issuePuzzle(endpoint: string): Promise<Puzzle | null> {
  try {
    const response = await fetch(`${endpoint}poc/issue`, {
      method: 'POST',
      signal: AbortSignal.timeout(POC_ISSUE_TIMEOUT_MS),
    });
    if (!response.ok) return null;

    return v.parse(puzzleSchema, await response.json());
  } catch {
    return null;
  }
}

/** Yield the main thread without the nested-timer clamp `setTimeout(…, 0)` would incur. */
function createYielder(): { yieldToEventLoop: () => Promise<void>; close: VoidFunction } {
  const channel = new MessageChannel();

  return {
    // Assignment rather than `addEventListener`: only this form starts the port implicitly.
    yieldToEventLoop: () =>
      new Promise<void>(resolve => {
        channel.port1.onmessage = () => resolve();
        channel.port2.postMessage(0);
      }),
    // A started port refs the event loop; an abandoned one would keep the process alive.
    close: () => {
      channel.port1.close();
      channel.port2.close();
    },
  };
}

/**
 * Search counters for one meeting `difficulty`, or `null` if the budget runs out first.
 *
 * Never throws, for the same reason `issuePuzzle` does not: a failure here must cost the header,
 * never the search.
 */
async function mine(sessionId: string, timestampMs: number, difficulty: number): Promise<number | null> {
  // Constructed inside the try: a throw from the constructor would otherwise escape a function
  // documented as never throwing, and the whole fail-safe rests on that promise holding.
  let yielder: ReturnType<typeof createYielder> | null = null;

  try {
    yielder = createYielder();
    const deadline = Date.now() + MINE_BUDGET_MS;

    for (let counter = 0; ; counter++) {
      if (peerSearchService.leadingZeroBits(sessionId, timestampMs, counter) >= difficulty) {
        return counter;
      }

      if (counter % MINE_CHUNK === MINE_CHUNK - 1) {
        if (Date.now() > deadline) {
          console.warn('[peer-search] proof-of-compute budget exhausted', { difficulty });

          return null;
        }

        await yielder.yieldToEventLoop();
      }
    }
  } catch {
    return null;
  } finally {
    yielder?.close();
  }
}

function createPeerResolver(backendUrl: string) {
  const identityEndpoint = new URL('/api/v1/', backendUrl).toString();

  // Set when the puzzle route does not answer, so a deployment with proof of compute switched off
  // is not charged a round trip per keystroke. Deliberately NOT permanent: a later 402 is the
  // server saying it wants a puzzle after all, and clears it — otherwise one transient failure
  // would demote the whole session to anonymous and every search after it would 402.
  let probeDisabled = false;
  // NOT sticky — cleared when the loop settles. There is no cancellation upstream (the search hook
  // debounces but never aborts an in-flight search), so without this a steadily typing user would
  // stack mining loops, each holding a share of the main thread until its budget expired.
  let mining = false;

  /** The header value for one search, or `null` — which is the ordinary case today. */
  async function solveProofOfCompute(): Promise<string | null> {
    if (probeDisabled || mining) return null;

    mining = true;
    try {
      const puzzle = await issuePuzzle(identityEndpoint);
      if (!puzzle) {
        probeDisabled = true;

        return null;
      }

      const counter = await mine(puzzle.sessionId, puzzle.timestamp, puzzle.difficulty);
      if (counter === null) return null;

      return peerSearchService.encodeProofOfComputeHeader({
        sessionId: puzzle.sessionId,
        timestampMs: puzzle.timestamp,
        difficulty: puzzle.difficulty,
        counter,
        checksum: puzzle.checksum,
      });
    } finally {
      mining = false;
    }
  }

  const getIdentity = async (address: string) => {
    try {
      return await consumerIdentityUseCase.getIdentity(address);
    } catch (error) {
      console.warn('[peer-resolver] identity lookup failed for %s (%s)', address, String(error));

      return null;
    }
  };

  return {
    async searchUsers(query: string): Promise<SearchResult[]> {
      return searchUsernames(identityEndpoint, query, await solveProofOfCompute(), () => {
        probeDisabled = false;
      });
    },

    async getPeerChatKey(address: string): Promise<Uint8Array | null> {
      const identity = await getIdentity(address);
      if (!identity?.identifierKey) {
        console.warn('[peer-resolver] no chat key resolved for %s', address);

        return null;
      }

      return fromHex(identity.identifierKey);
    },

    async getUsername(address: string): Promise<string | null> {
      const identity = await getIdentity(address);
      if (!identity) return null;

      return identity.fullUsername ?? identity.liteUsername;
    },

    async getPeerContact(address: string): Promise<{ chatKey: Uint8Array; username: string } | null> {
      const identity = await getIdentity(address);
      if (!identity?.identifierKey) return null;

      const username = identity.fullUsername ?? identity.liteUsername;
      if (!username) return null;

      return { chatKey: fromHex(identity.identifierKey), username };
    },
  };
}

export const peerGateway = {
  createPeerResolver,
};
