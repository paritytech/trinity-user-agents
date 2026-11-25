/**
 * `CallWindowLaunch` — the params the main process passes to the call window at
 * creation, carried over IPC. This is the ONE call type the main process needs,
 * so it lives in `@/shared` (the main process may import `@/shared` only, never a
 * renderer domain). The renderer-internal MessagePort message contract + its
 * validators live in `@/domains/chat/calls`. direction/purpose use inline
 * literals so this DTO stays dependency-free (structurally identical to the
 * domain's `CallDirection` / `CallPurpose`).
 */
export type CallWindowLaunch = {
  direction: 'outgoing' | 'incoming';
  purpose: 'audio' | 'video';
  offerId: string;
  peerName: string;
  // Chat session of the call's peer. Carried to the main renderer (via the full
  // launch object over IPC) so the bridge relays signals to/from the correct
  // contact instead of guessing sessions[0]. Optional because the call-window
  // renderer reconstructs a subset of the launch and never touches the chat transport.
  peerSessionId?: string;
};
