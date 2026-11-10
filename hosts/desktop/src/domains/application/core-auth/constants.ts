/**
 * Login is product-scoped traffic, but the user signs in before any product is
 * open. The host therefore keeps one pseudo-product provider of its own to carry
 * sign-in, cancellation, and disconnect — the same allocation dotli makes with its
 * landing auth host (`packages/ui/src/bridge.ts:122`, `:865`).
 *
 * The id has to satisfy host-spec C.7 — the core rejects anything that is not a
 * `.dot` name or a `localhost:` form. This is the host's own identity rather than a
 * resolvable app, and it scopes the core's account derivation, storage, and
 * permissions, so it MUST NOT be changed casually once a session exists.
 */
export const HOST_AUTH_PRODUCT_ID = 'polkadot-desktop.dot';
