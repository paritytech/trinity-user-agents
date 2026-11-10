// @vitest-environment happy-dom

import { Outlet, RouterProvider, createMemoryHistory, createRootRoute, createRoute, createRouter } from '@tanstack/react-router';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { type PropsWithChildren, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';
import { TranslationProvider } from '@/shared/translation';
import { sessionUseCase } from '@/domains/application';
import { browserTabs } from '@/aggregates/browser-tabs';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { type UserPopoverConnectionState, UserInfoPopover } from './UserInfoPopover';

// Both writes are use-case methods on plain objects, spied in place: the core session
// is never reached and the logout never tears anything down.
const disconnectMock = vi.spyOn(truapiRuntimeUseCase, 'disconnectSession').mockResolvedValue();
const performUserLogoutMock = vi.spyOn(sessionUseCase, 'performUserLogout').mockResolvedValue();

// Stable session fixture; the popover reads only the username fields.
const fakeSession: NonNullable<Parameters<typeof UserInfoPopover>[0]['session']> = {
  publicKey: `0x${'11'.repeat(32)}`,
  fullUsername: 'Goldie.89',
  liteUsername: 'Goldie',
};

const Providers = ({ children }: PropsWithChildren) => <TranslationProvider>{children}</TranslationProvider>;

// The popover navigates through `useNavigate`, so it renders inside a real router over
// an in-memory history; assertions read where the router went and what the tabs hold.
const renderOpen = async (
  state: UserPopoverConnectionState | 'no-connection',
  networkName = 'Paseo Next',
  session: Parameters<typeof UserInfoPopover>[0]['session'] = fakeSession,
  username = 'Goldie.89',
) => {
  const subject: ReactNode = (
    <Providers>
      <UserInfoPopover session={session} username={username} connectionState={state} networkName={networkName} defaultOpen>
        <button>trigger</button>
      </UserInfoPopover>
    </Providers>
  );
  const rootRoute = createRootRoute({
    component: () => (
      <>
        {subject}
        <Outlet />
      </>
    ),
  });
  const routeTree = rootRoute.addChildren(
    ['/', '/settings', '/onboarding'].map(path => createRoute({ getParentRoute: () => rootRoute, path })),
  );
  const router = createRouter({ routeTree, history: createMemoryHistory({ initialEntries: ['/'] }) });
  // A RouterProvider renders nothing until the router has matched its first location.
  await router.load();
  const { unmount } = render(<RouterProvider router={router} />);

  return { router, unmount };
};

describe('UserInfoPopover', () => {
  beforeEach(() => {
    disconnectMock.mockClear();
    performUserLogoutMock.mockClear();
  });

  it('renders connected banner when state=connected', async () => {
    await renderOpen('connected');
    const banner = screen.getByTestId(TEST_IDS.userPopoverBanner);
    expect(banner).toHaveTextContent('Connected to Paseo Next');
    expect(banner).toHaveTextContent('Desktop paired with Mobile');
  });

  it('uses the provided networkName in the banner title', async () => {
    await renderOpen('connected', 'Preview');
    expect(screen.getByTestId(TEST_IDS.userPopoverBanner)).toHaveTextContent('Connected to Preview');
  });

  it('renders reconnecting banner when state=reconnecting', async () => {
    await renderOpen('reconnecting');
    const banner = screen.getByTestId(TEST_IDS.userPopoverBanner);
    expect(banner).toHaveTextContent(/Reconnecting to Paseo Next/);
    expect(banner).toHaveTextContent('Attempting to reach the node again');
  });

  it('renders offline banner when state=offline', async () => {
    await renderOpen('offline');
    const banner = screen.getByTestId(TEST_IDS.userPopoverBanner);
    expect(banner).toHaveTextContent("You're offline");
    expect(banner).toHaveTextContent('Check internet connection and try again');
  });

  it('renders no-connection banner when state=no-connection', async () => {
    await renderOpen('no-connection', 'Paseo Next', null);
    const banner = screen.getByTestId(TEST_IDS.userPopoverBanner);
    expect(banner).toHaveTextContent('Not connected');
    expect(banner).toHaveTextContent('Log in to pair your account');
  });

  it('shows the 48px avatar in every state', async () => {
    for (const state of ['connected', 'reconnecting', 'offline'] as const) {
      const { unmount } = await renderOpen(state);
      expect(screen.getByTestId(TEST_IDS.userDisplayName)).toHaveTextContent('Goldie.89');
      expect(screen.getAllByText('G').length).toBeGreaterThan(0);
      unmount();
    }
  });

  it('opens the Settings tab when Settings row is clicked', async () => {
    const user = userEvent.setup();
    const { router } = await renderOpen('connected');
    await user.click(screen.getByTestId(TEST_IDS.userSettingsAction));
    expect(browserTabs.tabs$.get()).toContainEqual(
      expect.objectContaining({ id: 'settings', type: 'settings', deeplink: '', persistable: true }),
    );
    expect(browserTabs.selectedTabId$.get()).toBe('settings');
    await waitFor(() => expect(router.state.location.pathname).toBe('/settings'));
  });

  it('disconnects the core session when Log out is clicked', async () => {
    const user = userEvent.setup();
    const { router } = await renderOpen('connected');
    await user.click(screen.getByTestId(TEST_IDS.userLogoutButton));
    // On success the core drops the session and the session-teardown watcher
    // runs the logout — the component itself does NOT call performUserLogout.
    expect(disconnectMock).toHaveBeenCalled();
    expect(performUserLogoutMock).not.toHaveBeenCalled();
    expect(router.state.location.pathname).toBe('/');
  });

  it('leaves teardown to the watcher even when disconnect rejects', async () => {
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    disconnectMock.mockRejectedValueOnce(new Error('offline'));
    const user = userEvent.setup();
    await renderOpen('connected');
    await user.click(screen.getByTestId(TEST_IDS.userLogoutButton));
    // The core purges the session whether or not the wallet could be notified, so
    // a rejection here means the purge itself failed — reported, not worked
    // around. The component owns no teardown path of its own.
    await vi.waitFor(() => expect(consoleError).toHaveBeenCalled());
    expect(performUserLogoutMock).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it('dismisses when the toolbar overlay is pressed', async () => {
    await renderOpen('connected');
    expect(screen.getByTestId(TEST_IDS.userPopoverBanner)).toBeInTheDocument();
    // The overlay covers the toolbar's -webkit-app-region: drag area, which
    // otherwise swallows the press Radix needs to close the popover.
    fireEvent.pointerDown(screen.getByTestId(TEST_IDS.dismissOverlay));
    expect(screen.queryByTestId(TEST_IDS.userPopoverBanner)).toBeNull();
  });

  it('navigates to /onboarding when Log in is clicked for anonymous session', async () => {
    const user = userEvent.setup();
    const { router } = await renderOpen('no-connection', 'Paseo Next', null);
    await user.click(screen.getByTestId(TEST_IDS.userLogoutButton));
    expect(disconnectMock).not.toHaveBeenCalled();
    await waitFor(() => expect(router.state.location.pathname).toBe('/onboarding'));
  });
});
