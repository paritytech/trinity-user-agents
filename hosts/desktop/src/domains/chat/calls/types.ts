/**
 * Non-schema call types. The cross-renderer MessagePort bridge *message* types
 * (`WindowToMainMessage` / `MainToWindowMessage`) are schema-derived in
 * `schemas.ts` (the schema is the source of truth). The `CallWindowLaunch` DTO
 * the main process passes at window creation lives in `@/shared/call-bridge`
 * (the main process may import `@/shared` only).
 */

/**
 * Correlates the call window's `offerId` with the chat transport's `messageId`.
 * Outgoing calls store offerId → messageId once `sendMessage` resolves; for
 * incoming calls the offerId IS the chat messageId, so the two match.
 */
export type OfferIdMap = Map<string, string>; // offerId → chatMessageId
