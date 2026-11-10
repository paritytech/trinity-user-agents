// @vitest-environment happy-dom
import { createMemoryHistory, createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { waitFor } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { isSystemTabType, navigationUseCase } from './navigationUseCase';
import { browserTabs } from './state/tabs';

// A real router over an in-memory history: the use case reads its location and asks it
// to navigate, so the assertion is on where the router ended up, not on a call.
async function routerAt(pathname: string) {
  const rootRoute = createRootRoute();
  const routeTree = rootRoute.addChildren(
    ['/', '/chat/{-$chatId}', '/dashboard'].map(path => createRoute({ getParentRoute: () => rootRoute, path })),
  );
  const router = createRouter({ routeTree, history: createMemoryHistory({ initialEntries: [pathname] }) });
  await router.load();

  // The use case is typed against the registered app router; a three-route memory router
  // has the same `state.location` and `navigate` shape, which is all the use case reads.
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- see above
  return router as unknown as Parameters<typeof navigationUseCase.syncRouteToSelectedTab>[0];
}

describe('isSystemTabType', () => {
  it('returns true for chat, settings, and dashboard', () => {
    expect(isSystemTabType('chat')).toBe(true);
    expect(isSystemTabType('settings')).toBe(true);
    expect(isSystemTabType('dashboard')).toBe(true);
  });

  it('returns false for product and new-tab tabs', () => {
    expect(isSystemTabType('product')).toBe(false);
    expect(isSystemTabType('new-tab')).toBe(false);
  });
});

describe('syncRouteToSelectedTab', () => {
  it('navigates to dashboard when dashboard tab is selected off-route', async () => {
    const router = await routerAt('/chat');
    browserTabs.addTab({ id: 'dashboard', type: 'dashboard', deeplink: '' }, { persistable: true });
    browserTabs.selectTab('dashboard');

    navigationUseCase.syncRouteToSelectedTab(router);

    await waitFor(() => expect(router.state.location.pathname).toBe('/dashboard'));
  });

  it('skips navigation when already on the tab route', async () => {
    const router = await routerAt('/chat');
    browserTabs.addTab({ id: 'chat', type: 'chat', deeplink: '' }, { persistable: true });
    browserTabs.selectTab('chat');

    navigationUseCase.syncRouteToSelectedTab(router);

    expect(router.state.location.pathname).toBe('/chat');
  });
});

describe('navigateHomeWhenNoTabs', () => {
  it('navigates to dashboard when tabs are empty on a feature route', async () => {
    const router = await routerAt('/chat');

    navigationUseCase.navigateHomeWhenNoTabs(router);

    await waitFor(() => expect(router.state.location.pathname).toBe('/dashboard'));
  });

  it('skips navigation when already on dashboard', async () => {
    const router = await routerAt('/dashboard');

    navigationUseCase.navigateHomeWhenNoTabs(router);

    expect(router.state.location.pathname).toBe('/dashboard');
  });
});
