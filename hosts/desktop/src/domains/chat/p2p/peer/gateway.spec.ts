import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { peerGateway } from './gateway';

// The resolver takes its base URL as a parameter, so nothing here stubs the environment.
const BASE = 'https://identity.example';
const SEARCH_URL = `${BASE}/api/v1/usernames/search?prefix=al&limit=50`;

const ROW = { accountId: '5F7BRK', username: 'alicia.01', status: 'ASSIGNED', createdAt: 'x', updatedAt: 'y' };

// A puzzle the schema accepts: real hyphenated UUID, non-negative timestamp, 64 lowercase hex.
const SESSION = '1b9d6bcd-bbfd-4b2d-9b5d-ab8dfbbd4bed';
const CHECKSUM = 'c8828951fd6c123fdbf6501f111d27dd3f260839344a7370e0dd8f20e2c40482';

const json = (body: unknown, status = 200) => ({ ok: status < 400, status, json: () => Promise.resolve(body) });
const failure = (status: number) => ({ ok: false, status, text: () => Promise.resolve('') });

/**
 * `searchUsers` calls `fetch` twice — the puzzle probe, then the search — so the stub branches on
 * URL. Reading `mock.calls[0]` would read the probe, not the search.
 */
function stubFetch(puzzleResponse: unknown, searchResponse: unknown = json({ usernames: [ROW], nextCursor: null })) {
  vi.stubGlobal(
    'fetch',
    vi.fn((url: unknown) => Promise.resolve(String(url).includes('poc/issue') ? puzzleResponse : searchResponse)),
  );
}

const searchCall = () => vi.mocked(fetch).mock.calls.find(([url]) => String(url).includes('usernames/search'));

// Each case builds its own resolver: the disabled-probe flag is per-resolver and sticky, so a
// shared one would let the first case switch proof of compute off for every case after it.
const resolver = () => peerGateway.createPeerResolver(BASE);

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('peerGateway search request', () => {
  beforeEach(() => {
    stubFetch(failure(404));
  });

  it('requests the search endpoint with the prefix and page size', async () => {
    await resolver().searchUsers('al');

    expect(searchCall()?.[0]).toBe(SEARCH_URL);
  });

  // The envelope is unwrapped and the wire's extra fields are dropped: `createdAt`/`updatedAt`
  // are in the payload, undeclared in the schema, and so absent from the domain value.
  it('unwraps the envelope into search results', async () => {
    await expect(resolver().searchUsers('al')).resolves.toEqual([
      { accountId: '5F7BRK', username: 'alicia.01', status: 'ASSIGNED' },
    ]);
  });

  // Both strings reach the user verbatim through the search panel's error slot.
  it('reports a rate limit in words rather than a status code', async () => {
    stubFetch(failure(404), failure(429));

    await expect(resolver().searchUsers('al')).rejects.toThrow('Too many searches. Try again in a moment.');
  });

  it('reports a proof-of-compute refusal without naming proof of compute', async () => {
    stubFetch(failure(404), failure(402));

    await expect(resolver().searchUsers('al')).rejects.toThrow('Search is temporarily unavailable.');
  });
});

describe('peerGateway proof of compute', () => {
  // The regression that protects today's behaviour: the route is not mounted on deployments with
  // proof of compute disabled, and search must keep working anonymously.
  it('searches without a header when no puzzle is offered', async () => {
    stubFetch(failure(404));

    await expect(resolver().searchUsers('al')).resolves.toHaveLength(1);
    expect(searchCall()?.[1]?.headers).toEqual({ Accept: 'application/json' });
  });

  it('solves a puzzle and attaches the header', async () => {
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_000, difficulty: 1, checksum: CHECKSUM }, 201));

    await resolver().searchUsers('al');

    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- HeadersInit is not indexable
    const header = (searchCall()?.[1]?.headers as Record<string, string>)['Proof-Of-Compute'];
    expect(header).toBeDefined();

    const fields = atob(header!).split(':');
    expect(fields).toHaveLength(5);
    expect(fields[0]).toBe(SESSION);
    // Difficulty and checksum are re-HMACed by the server, so they must round-trip unchanged.
    expect(fields[2]).toBe('1');
    expect(fields[4]).toBe(CHECKSUM);
    // Standard padded base64 — `atob` above already rejects the base64url alphabet.
    expect(header!.length % 4).toBe(0);
  });

  // Difficulty 1 solves within a couple of hashes and never reaches the yield boundary. This
  // vector needs 16,763 counters, crossing it four times, so the MessageChannel path is exercised.
  it('mines across event-loop yields at a realistic difficulty', async () => {
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_040, difficulty: 16, checksum: CHECKSUM }, 201));

    await resolver().searchUsers('al');

    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- HeadersInit is not indexable
    const header = (searchCall()?.[1]?.headers as Record<string, string>)['Proof-Of-Compute'];
    expect(atob(header!).split(':')[3]).toBe('16763');
  });

  it('rejects a malformed puzzle and searches anonymously', async () => {
    stubFetch(json({ sessionId: 'not-a-uuid', timestamp: -1, difficulty: 99, checksum: 'short' }, 201));

    await expect(resolver().searchUsers('al')).resolves.toHaveLength(1);
    expect(searchCall()?.[1]?.headers).toEqual({ Accept: 'application/json' });
  });

  // The flag is per-resolver and sticky, so a second search must not re-probe a route that 404ed.
  it('stops probing once a deployment declines to issue a puzzle', async () => {
    stubFetch(failure(404));
    const peer = resolver();

    await peer.searchUsers('al');
    await peer.searchUsers('al');

    expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes('poc/issue'))).toHaveLength(1);
  });

  // `mining` must be cleared in a finally: if it leaked, the first search would carry a header and
  // every one after it would silently fall back to anonymous.
  it('attaches a header on consecutive searches', async () => {
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_000, difficulty: 1, checksum: CHECKSUM }, 201));
    const peer = resolver();

    await peer.searchUsers('al');
    await peer.searchUsers('al');

    const headers = vi
      .mocked(fetch)
      .mock.calls.filter(([url]) => String(url).includes('usernames/search'))
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- HeadersInit is not indexable
      .map(([, init]) => (init?.headers as Record<string, string>)['Proof-Of-Compute']);

    expect(headers).toHaveLength(2);
    expect(headers.every(Boolean)).toBe(true);
  });
});

describe('peerGateway proof-of-compute resilience', () => {
  // Without a bound, a deployment that raises difficulty would hang a type-ahead search. The
  // budget is checked at a chunk boundary, so difficulty 32 is used to guarantee one is reached.
  it('gives up on an unaffordable puzzle and searches anonymously', async () => {
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_000, difficulty: 32, checksum: CHECKSUM }, 201));
    // Jump the clock past the deadline rather than burn 1.5s of wall time; the loop reads
    // `Date.now()` only to set and test the deadline.
    const realNow = Date.now();
    vi.spyOn(Date, 'now')
      .mockReturnValueOnce(realNow)
      .mockReturnValue(realNow + 10_000);

    await expect(resolver().searchUsers('al')).resolves.toHaveLength(1);
    expect(searchCall()?.[1]?.headers).toEqual({ Accept: 'application/json' });
  });

  // No cancellation exists upstream, so a second search starting while the first is still mining
  // must not start a second loop — it sends its request header-less instead.
  it('does not start a second mining loop while one is in flight', async () => {
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_040, difficulty: 16, checksum: CHECKSUM }, 201));
    const peer = resolver();

    const [first, second] = await Promise.all([peer.searchUsers('al'), peer.searchUsers('al')]);

    expect(first).toHaveLength(1);
    expect(second).toHaveLength(1);
    // One puzzle issued, not two: the second search skipped the probe entirely.
    expect(vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes('poc/issue'))).toHaveLength(1);
  });

  // A transient failure must not demote the session forever: a 402 is the server saying it does
  // want a puzzle, which re-enables probing.
  it('resumes probing after the server reports a puzzle is required', async () => {
    const peer = resolver();

    // First search: the probe fails, so no header — and the server refuses with 402.
    stubFetch(failure(503), failure(402));
    await expect(peer.searchUsers('al')).rejects.toThrow('Search is temporarily unavailable.');

    // Second search: the probe is tried again rather than skipped, and now succeeds.
    stubFetch(json({ sessionId: SESSION, timestamp: 1_700_000_000_000, difficulty: 1, checksum: CHECKSUM }, 201));
    await peer.searchUsers('al');

    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- HeadersInit is not indexable
    expect((searchCall()?.[1]?.headers as Record<string, string>)['Proof-Of-Compute']).toBeDefined();
  });
});
