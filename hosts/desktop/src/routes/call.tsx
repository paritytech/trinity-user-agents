import { createFileRoute } from '@tanstack/react-router';

import { CallWindowScreen } from '@/features/call';

/**
 * The call window loads this route in its own BrowserWindow. It is a SIBLING of
 * the `_app` pathless layout, so it runs no bootstrap, no PappProvider, and no
 * chat binding — the call window shares this bundle but runs none of the app.
 *
 * All call data (launch params + iceConfig + the MessagePort) arrives via the
 * `__callInit` handshake — captured eagerly by `callInitPort` (imported in
 * src/index.tsx) and read inside `CallWindowScreen`. Nothing comes from the URL,
 * so the route needs no params.
 */
export const Route = createFileRoute('/call')({
  component: CallWindowScreen,
});
