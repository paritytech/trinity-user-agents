/**
 * The identity registry — the authority on whether a username claim actually landed,
 * and the same source the app's contact search reads. A claim this module confirms is
 * therefore one the UI can find.
 *
 * Reads only. `truapi-host` performs the claim; nothing here writes.
 *
 * Which deployment to ask is infrastructure, so it arrives as `E2E_IDENTITY_URL` and
 * appears nowhere in this repository. Unset means the registry is simply not consulted
 * — callers then fall back to the name `truapi-host` reported on its own startup line,
 * which already carries the backend-assigned suffix. That is enough to drive the suite;
 * what is lost is the independent confirmation that the registration is visible to the
 * indexer the UI queries.
 *
 * The URL MUST address the registry for the network under test. Nothing here can check
 * that: a registry for another chain answers perfectly well and reports every claim as
 * missing, which reads as a provisioning failure.
 */

import { errorMessage } from './errors';

/** One page is plenty: the caller searches a full username, not a broad prefix. */
const SEARCH_PAGE_SIZE = 100;

const SEARCH_TIMEOUT_MS = 10_000;

export type RegisteredUsername = {
  accountId: string;
  /** The registered lite username, backend-assigned digits included (`testbot….23`). */
  username: string;
  status: string;
  /** ISO-8601. Read only to order registrations — see `findRegistrations`. */
  createdAt: string;
};

export function identityBackendUrl(): string | undefined {
  return process.env['E2E_IDENTITY_URL'] || undefined;
}

/**
 * Said once per process, not per lookup: the poll loop asks for every user every few
 * seconds, and a run that has quietly lost registry confirmation should say so exactly
 * one time rather than either shouting or staying silent.
 */
let announcedMissingEndpoint = false;

function announceMissingEndpoint(): void {
  if (announcedMissingEndpoint) return;
  announcedMissingEndpoint = true;
  console.info('[identity] E2E_IDENTITY_URL is unset — registrations are not confirmed against the registry');
}

/**
 * Whether `username` is a registration of `base` — `base` itself, or `base` plus the
 * numeric suffix the backend assigns (`testbot….23`).
 *
 * The endpoint matches by prefix, so a search for `truapitesta` would also return
 * `truapitestabbb`. Exported for unit testing.
 */
export function matchesBase(username: string, base: string): boolean {
  if (username === base) return true;
  if (!username.startsWith(`${base}.`)) return false;

  const digits = username.slice(base.length + 1);

  return digits.length > 0 && /^\d+$/.test(digits);
}

function isRegisteredUsername(row: unknown): row is RegisteredUsername {
  if (typeof row !== 'object' || row === null) return false;

  return (
    'accountId' in row &&
    typeof row.accountId === 'string' &&
    'username' in row &&
    typeof row.username === 'string' &&
    'status' in row &&
    typeof row.status === 'string' &&
    'createdAt' in row &&
    typeof row.createdAt === 'string'
  );
}

/** Newest first; a row with an unreadable timestamp sorts last rather than winning by accident. */
function byNewest(a: RegisteredUsername, b: RegisteredUsername): number {
  const left = Date.parse(a.createdAt);
  const right = Date.parse(b.createdAt);

  return (Number.isNaN(right) ? -Infinity : right) - (Number.isNaN(left) ? -Infinity : left);
}

type FetchLike = typeof globalThis.fetch;

/**
 * Registrations of `base`, **newest first**.
 *
 * Order matters because a name can hold several: a base picks up a new registration
 * every time an identity under it is re-provisioned, and the backend returns them
 * ordered by assigned digits, not by age. Only the newest belongs to the account the
 * slot currently holds.
 *
 * `null` means **could not tell** — no registry configured, the request failed, or the
 * deployment demands a `Proof-Of-Compute` header this harness does not mine (402; the
 * app's miner lives in `src/domains/chat/p2p/peer/service.ts`). Callers MUST read null
 * as "no answer", never as "not registered": an empty array is the only statement that
 * the claim has not landed.
 *
 * `doFetch` is a seam for unit tests — the module takes its collaborator as a
 * parameter rather than being swapped out wholesale with `vi.mock`.
 */
export async function findRegistrations(
  base: string,
  doFetch: FetchLike = globalThis.fetch,
): Promise<RegisteredUsername[] | null> {
  const endpoint = identityBackendUrl();
  if (!endpoint) {
    announceMissingEndpoint();

    return null;
  }

  const params = new URLSearchParams({ prefix: base, limit: String(SEARCH_PAGE_SIZE) });
  const url = `${endpoint.replace(/\/+$/, '')}/api/v1/usernames/search?${params.toString()}`;

  try {
    const res = await doFetch(url, {
      method: 'GET',
      headers: { Accept: 'application/json' },
      signal: AbortSignal.timeout(SEARCH_TIMEOUT_MS),
    });
    if (!res.ok) {
      console.warn(`[identity] search "${base}" -> ${res.status}, cannot confirm the registration`);

      return null;
    }

    const payload: unknown = await res.json();
    const rows = typeof payload === 'object' && payload !== null && 'usernames' in payload ? payload.usernames : undefined;
    if (!Array.isArray(rows)) {
      console.warn(`[identity] search "${base}" returned an unexpected payload, cannot confirm the registration`);

      return null;
    }

    return rows
      .filter(isRegisteredUsername)
      .filter(row => matchesBase(row.username, base))
      .sort(byNewest);
  } catch (err) {
    console.warn(`[identity] search "${base}": ${errorMessage(err)}, cannot confirm the registration`);

    return null;
  }
}
