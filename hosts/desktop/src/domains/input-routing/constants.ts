// RFC-0027 § Bounds. Only the *presence* of each bound is normative; the values
// are the RFC's recommended defaults and are explicitly unvalidated (Q12).

/**
 * `User` text starts a round only after this long without further input, so a
 * round costs a pause rather than a keystroke.
 */
export const DEBOUNCE_MS = 250;

/** At most N products are queried from one context set. */
export const CONTEXT_CAP = 16;

/** Outstanding subscriptions are interrupted; later responses are discarded. */
export const HARD_DEADLINE_MS = 3_000;

/** At most M candidates are taken from each response; the excess is discarded. */
export const CANDIDATE_CAP = 8;

/** A response over B encoded bytes is rejected whole, not truncated. */
export const PAYLOAD_CAP_BYTES = 64 * 1024;

/**
 * An attachment larger than this is declined before delivery. The RFC gives no
 * recommended value — it follows from how large a message this host can hold and
 * carry across its product boundary. This is a desktop host.
 */
export const ATTACHMENT_CAP_BYTES = 32 * 1024 * 1024;

/** What a host substitutes when the OS hands over an attachment with no type. */
export const DEFAULT_MIME_TYPE = 'application/octet-stream';
