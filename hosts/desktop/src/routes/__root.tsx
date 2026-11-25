import { Outlet, createRootRoute } from '@tanstack/react-router';

import { RouteErrorFallback } from '@/shared/components';

/**
 * The router root renders for EVERY window (main app and the `/call` call
 * window), so it must stay minimal — just an `<Outlet/>` and a shared error
 * boundary. The entire main-app shell (bootstrap, PappProvider, chat binding,
 * AppShell, Browser, deep-link nav) lives in the pathless `_app` layout route,
 * so the sibling `/call` route inherits none of it.
 */
export const Route = createRootRoute({
  component: () => <Outlet />,
  errorComponent: RouteErrorFallback,
});
