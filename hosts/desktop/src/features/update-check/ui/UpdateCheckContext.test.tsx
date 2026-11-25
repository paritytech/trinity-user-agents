// @vitest-environment happy-dom

import { toast, toastError, toastSuccess } from '@novasamatech/tr-ui';
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import {
  clearDismissedVersion,
  clearPendingVersion,
  readDismissedVersion,
  readPendingVersion,
  writeDismissedVersion,
  writePendingVersion,
} from '../state/updateCheck';

import { UpdateCheckProvider, useUpdateCheck } from './UpdateCheckContext';

// Toasts are driven imperatively through the design-system helpers; mock only those, keep the
// real Dialog/Button so the modal still renders. `vi.mock` is hoisted above the imports, so the
// toast helpers imported above resolve to these mocks.
vi.mock('@novasamatech/tr-ui', async importOriginal => {
  const actual = await importOriginal<Record<string, unknown>>();
  return { ...actual, toast: vi.fn(), toastSuccess: vi.fn(), toastError: vi.fn() };
});

type UpdateEventCallback = (event: { type: string; data?: unknown }) => void;
type CheckForUpdatesRequestCallback = () => void;

let updateEventCallback: UpdateEventCallback;
let checkForUpdatesRequestCallback: CheckForUpdatesRequestCallback;

const mockCheckForUpdates = vi.fn().mockResolvedValue(undefined);
const mockQuitAndInstall = vi.fn();
const mockGetHostMetadata = vi.fn().mockResolvedValue({ hostVersion: '2.1.0' });
const mockOnUpdateEvent = vi.fn((cb: UpdateEventCallback) => {
  updateEventCallback = cb;
  return vi.fn();
});
const mockOnCheckForUpdatesRequest = vi.fn((cb: CheckForUpdatesRequestCallback) => {
  checkForUpdatesRequestCallback = cb;
  return vi.fn();
});

function mockWindowApp(overrides?: Partial<typeof window.App>) {
  Object.defineProperty(window, 'App', {
    value: {
      isAutoUpdateSupported: true,
      checkForUpdates: mockCheckForUpdates,
      quitAndInstall: mockQuitAndInstall,
      getHostMetadata: mockGetHostMetadata,
      onUpdateEvent: mockOnUpdateEvent,
      onCheckForUpdatesRequest: mockOnCheckForUpdatesRequest,
      ...overrides,
    },
    writable: true,
    configurable: true,
  });
}

const CheckButton = () => {
  const { openUpdateCheck } = useUpdateCheck();
  return <button onClick={openUpdateCheck}>Check</button>;
};

// The startup reconciliation hangs off `getHostMetadata()`. Await that one promise
// so the state it sets lands inside `act`, rather than draining whatever happens to
// be queued.
async function renderProvider() {
  const result = render(
    <TranslationProvider>
      <UpdateCheckProvider>
        <CheckButton />
      </UpdateCheckProvider>
    </TranslationProvider>,
  );
  await act(async () => {
    await mockGetHostMetadata.mock.results.at(-1)?.value;
  });

  return result;
}

function emitUpdateEvent(type: string, data?: unknown) {
  act(() => {
    updateEventCallback({ type, data });
  });
}

// Last options object passed to the mocked raw `toast` (the ready reminder).
function lastToastOptions() {
  const calls = vi.mocked(toast).mock.calls;
  return calls[calls.length - 1]?.[1] as {
    description?: string;
    onDismiss?: () => void;
    action?: { label: string; onClick: () => void };
  };
}

describe('UpdateCheckContext', () => {
  beforeEach(() => {
    mockCheckForUpdates.mockClear();
    mockQuitAndInstall.mockClear();
    mockGetHostMetadata.mockReset();
    mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.1.0' });
    mockOnUpdateEvent.mockClear();
    mockOnCheckForUpdatesRequest.mockClear();
    vi.mocked(toast).mockClear();
    vi.mocked(toastSuccess).mockClear();
    vi.mocked(toastError).mockClear();
    // The persisted version state is a module-level singleton, so reset it in memory between tests
    // (localStorage.clear() alone would not — the binding only syncs from storage once at import).
    clearPendingVersion();
    clearDismissedVersion();
    localStorage.clear();
    mockWindowApp();
  });

  afterEach(() => {
    // @ts-expect-error — cleanup global
    delete window.App;
  });

  describe('manual check via button', () => {
    it('opens modal and calls checkForUpdates when button is clicked', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));

      expect(mockCheckForUpdates).toHaveBeenCalledOnce();
      expect(screen.getByText('Checking for update…')).toBeTruthy();
    });

    it('does nothing when checkForUpdates is unavailable', async () => {
      mockWindowApp({ checkForUpdates: undefined });
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));

      expect(mockCheckForUpdates).not.toHaveBeenCalled();
    });
  });

  describe('manual check events (modal flow)', () => {
    it('shows downloading in modal on update-available after manual check', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-available', { version: '2.1.0' });

      expect(screen.getByText('Downloading update…')).toBeTruthy();
    });

    it('shows ready-to-install in modal on update-downloaded after manual check', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      expect(screen.getByText('Update is ready')).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Relaunch' })).toBeTruthy();
      // Persisted so the reminder survives if the user does not relaunch now.
      expect(readPendingVersion()).toBe('2.1.0');
    });

    it('shows error in modal after manual check', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('error', { message: 'Network timeout' });

      expect(screen.getByText('Network timeout')).toBeTruthy();
    });

    it('shows honest unsupported state on update-unsupported after manual check', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-unsupported');

      expect(screen.getByText("Updates aren't available")).toBeTruthy();
    });
  });

  describe('auto-check events (toast flow)', () => {
    it('shows the ready toast on update-downloaded from auto-check', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      expect(toast).toHaveBeenCalledWith('New Polkadot Desktop update!', expect.objectContaining({}));
      expect(lastToastOptions().description).toBe('Version 2.1.0 is available');
      expect(readPendingVersion()).toBe('2.1.0');
    });

    it('does not show toast when version is dismissed', async () => {
      writeDismissedVersion('2.1.0');
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      expect(toast).not.toHaveBeenCalled();
      // Still persisted as pending even though the reminder is suppressed.
      expect(readPendingVersion()).toBe('2.1.0');
    });

    it('shows toast when a different version is dismissed', async () => {
      writeDismissedVersion('1.9.0');
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      expect(toast).toHaveBeenCalledOnce();
    });

    it('does not open the modal on auto-check download', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      expect(screen.queryByRole('button', { name: 'Relaunch' })).toBeNull();
      expect(screen.queryByRole('button', { name: 'Later' })).toBeNull();
    });

    it('stays silent on a background error', async () => {
      await renderProvider();

      emitUpdateEvent('error', { message: 'boom' });

      expect(toast).not.toHaveBeenCalled();
      expect(screen.queryByText('Update failed')).toBeNull();
    });
  });

  describe('ready toast behavior', () => {
    it('persists the dismissed version through the toast onDismiss', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      act(() => {
        lastToastOptions().onDismiss?.();
      });

      expect(readDismissedVersion()).toBe('2.1.0');
    });

    it('relaunches from the toast action', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      act(() => {
        lastToastOptions().action?.onClick();
      });

      expect(mockQuitAndInstall).toHaveBeenCalledOnce();
    });
  });

  describe('install actions (modal)', () => {
    it('calls quitAndInstall when Relaunch is clicked', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-downloaded', { version: '2.1.0' });

      await userEvent.click(screen.getByRole('button', { name: 'Relaunch' }));
      expect(mockQuitAndInstall).toHaveBeenCalledOnce();
    });

    it('Later closes the modal and hands off to the persistent toast', async () => {
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      expect(screen.getByText('Update is ready')).toBeTruthy();
      vi.mocked(toast).mockClear();

      await userEvent.click(screen.getByRole('button', { name: 'Later' }));

      expect(screen.queryByText('Update is ready')).toBeNull();
      expect(toast).toHaveBeenCalledOnce();
      expect(lastToastOptions().description).toBe('Version 2.1.0 is available');
    });

    it('Later re-arms the reminder even for a previously-dismissed version', async () => {
      writeDismissedVersion('2.1.0');
      await renderProvider();

      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      vi.mocked(toast).mockClear();

      await userEvent.click(screen.getByRole('button', { name: 'Later' }));

      // Explicit "Later" is an active deferral — it re-shows the toast despite the earlier dismissal.
      expect(toast).toHaveBeenCalledOnce();
    });

    it('a failed modal relaunch does not leak into a later background error toast', async () => {
      await renderProvider();

      // Manual check → ready → Relaunch fails → modal shows error, then user closes it.
      await userEvent.click(screen.getByText('Check'));
      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      await userEvent.click(screen.getByRole('button', { name: 'Relaunch' }));
      emitUpdateEvent('error', { message: 'signature' });
      // Close the modal (status resets, relaunch flag cleared).
      const closeButtons = screen.getAllByRole('button', { name: 'Close' });
      const lastClose = closeButtons[closeButtons.length - 1];
      if (lastClose) await userEvent.click(lastClose);

      // A later unrelated background check error must NOT pop an install-failure toast.
      emitUpdateEvent('error', { message: 'network blip' });
      expect(toastError).not.toHaveBeenCalled();
    });
  });

  describe('menu request', () => {
    it('opens modal when onCheckForUpdatesRequest fires', async () => {
      await renderProvider();

      act(() => {
        checkForUpdatesRequestCallback();
      });

      expect(mockCheckForUpdates).toHaveBeenCalledOnce();
      expect(screen.getByText('Checking for update…')).toBeTruthy();
    });
  });

  describe('restores last event on manual open', () => {
    it('restores update-not-available without re-checking', async () => {
      await renderProvider();

      emitUpdateEvent('update-not-available');
      await userEvent.click(screen.getByText('Check'));

      expect(screen.getByText('Polkadot Desktop is up to date!')).toBeTruthy();
      expect(mockCheckForUpdates).not.toHaveBeenCalled();
    });

    it('restores update-downloaded without re-checking', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      await userEvent.click(screen.getByText('Check'));

      expect(screen.getByText('Update is ready')).toBeTruthy();
      expect(mockCheckForUpdates).not.toHaveBeenCalled();
    });

    it('restores update-unsupported without re-checking', async () => {
      await renderProvider();

      emitUpdateEvent('update-unsupported');
      await userEvent.click(screen.getByText('Check'));

      expect(screen.getByText("Updates aren't available")).toBeTruthy();
      expect(mockCheckForUpdates).not.toHaveBeenCalled();
    });
  });

  describe('post-relaunch confirmation', () => {
    it('confirms a completed update when the pending download is now running', async () => {
      writePendingVersion('2.1.0');
      mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.1.0' });

      await renderProvider();

      expect(toastSuccess).toHaveBeenCalledWith(
        expect.objectContaining({ title: 'Polkadot Desktop updated!', description: 'Version 2.1.0 installed' }),
      );
      expect(readPendingVersion()).toBeNull();
      expect(toast).not.toHaveBeenCalled();
    });

    it('does not falsely confirm a downgrade or reinstall with no matching pending', async () => {
      // Rolled back 2.1.0 -> 2.0.0; nothing was pending, so no "updated" toast.
      mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.0.0' });

      await renderProvider();

      expect(toastSuccess).not.toHaveBeenCalled();
    });

    it('does not confirm on first launch', async () => {
      mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.1.0' });

      await renderProvider();

      expect(toastSuccess).not.toHaveBeenCalled();
    });
  });

  describe('startup reconciliation of a pending update', () => {
    it('does not re-prompt a strictly-newer pending on launch (waits for an armed re-download)', async () => {
      // quitAndInstall can only install a download staged in the current session, so the reminder is
      // re-shown by the updater re-emitting update-downloaded, not by reconciliation.
      writePendingVersion('2.1.0');
      mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.0.0' });

      await renderProvider();

      expect(toast).not.toHaveBeenCalled();
      // Pending is retained so the re-emitted download is recognised (and can still be confirmed).
      expect(readPendingVersion()).toBe('2.1.0');
    });

    it('drops a stale pending older than the running build (no downgrade prompt)', async () => {
      writePendingVersion('2.0.0');
      mockGetHostMetadata.mockResolvedValue({ hostVersion: '2.1.0' });

      await renderProvider();

      expect(toast).not.toHaveBeenCalled();
      expect(readPendingVersion()).toBeNull();
    });
  });

  describe('install failure feedback', () => {
    it('surfaces an error toast when a relaunch from the toast fails', async () => {
      await renderProvider();

      emitUpdateEvent('update-downloaded', { version: '2.1.0' });
      act(() => {
        lastToastOptions().action?.onClick();
      });
      // quitAndInstall failed -> main emits an error while no manual check is active.
      emitUpdateEvent('error', { message: 'Code signature failed' });

      expect(toastError).toHaveBeenCalledOnce();
    });

    it('stays silent on a background error when no relaunch was requested', async () => {
      await renderProvider();

      emitUpdateEvent('error', { message: 'network' });

      expect(toastError).not.toHaveBeenCalled();
    });
  });
});
