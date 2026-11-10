import * as v from 'valibot';

// Must stay in sync with the `Network` union accepted by host-chat's
// `createAccountService` (@novasamatech/host-chat/dist/accountService.d.ts).
// `hostChatNetwork` arrives from environment config — a trust boundary —
// so it is validated against the allowed set instead of being cast.
export const HostChatNetworkSchema = v.picklist(['paseo-next', 'paseo-next-v2', 'preview', 'stable', 'release', 'summit']);

/**
 * The identity backend's username-search response — an HTTP trust boundary, so it is validated
 * rather than trusted. The payload is an envelope, not a bare array, and `nextCursor` is declared
 * to record that it pages even though this host reads one page. Extra fields the backend sends
 * (`createdAt`, `updatedAt`) are ignored; a row missing one this host reads fails the whole parse,
 * which is Valibot's behaviour for an array of objects and is the honest outcome — a half-read
 * result list is worse than a reported failure.
 */
export const searchResponseSchema = v.object({
  usernames: v.array(
    v.object({
      accountId: v.string(),
      username: v.string(),
      status: v.string(),
    }),
  ),
  nextCursor: v.optional(v.nullable(v.string())),
});

/**
 * A proof-of-compute puzzle, as issued by `POST /api/v1/poc/issue`.
 *
 * Every constraint here is one the server re-checks when it parses the solved header, so a puzzle
 * that fails them could only ever produce a 400. `sessionId` matters most: the work preimage
 * hex-parses it, so a non-UUID would yield a silently wrong digest rather than an error.
 */
export const puzzleSchema = v.object({
  sessionId: v.pipe(v.string(), v.uuid()),
  timestamp: v.pipe(v.number(), v.integer(), v.minValue(0), v.maxValue(8_640_000_000_000_000)),
  difficulty: v.pipe(v.number(), v.integer(), v.minValue(1), v.maxValue(32)),
  // Not `v.hexadecimal()`: that accepts uppercase and an optional `0x` prefix.
  checksum: v.pipe(v.string(), v.regex(/^[0-9a-f]{64}$/)),
});

export type Puzzle = v.InferOutput<typeof puzzleSchema>;
