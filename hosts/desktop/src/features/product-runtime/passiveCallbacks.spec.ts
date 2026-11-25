// @vitest-environment happy-dom

import { of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { saveLocale } from '@/shared/translation';
import { themeResource } from '@/domains/application';

const openExternal = vi.fn();
const scheduleNotification = vi.fn((_request: { text: string }) => Promise.resolve({ ok: true, id: 7 }));
const cancelNotification = vi.fn(() => Promise.resolve());
const getSystemDevicePermissionStatus = vi.fn();

const App = {
  scheduleNotification,
  cancelNotification,
  onNotificationActivated: () => () => {},
  focusWindow: vi.fn(),
  getSystemDevicePermissionStatus,
};

vi.stubGlobal('window', Object.assign(window, { App, open: openExternal }));

import { createPassiveCallbacks } from './passiveCallbacks';

const navigateMock = vi.fn();

describe('navigation callback', () => {
  beforeEach(() => {
    navigateMock.mockClear();
    openExternal.mockClear();
  });

  it('routes a dotNS url into the app instead of the system browser', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await result.current.navigation.navigateTo('polkadot://demo.dot');

    expect(navigateMock).toHaveBeenCalledOnce();
    expect(openExternal).not.toHaveBeenCalled();
  });

  // The remote-access decision is the core's, taken through the product-scoped
  // `permissions.remotePermission` callback before this one is ever called. By the
  // time a URL reaches here it is already approved, so this opens it unconditionally.
  // The gate itself is covered with `remotePermission` in usePromptCallbacks.
  it('hands an approved external url to the system browser', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await result.current.navigation.navigateTo('https://example.com');

    expect(openExternal).toHaveBeenCalledWith('https://example.com', '_blank');
  });
});

describe('notification callbacks', () => {
  beforeEach(() => {
    scheduleNotification.mockClear();
    cancelNotification.mockClear();
  });

  it('returns the host-assigned id', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await expect(result.current.notifications.pushNotification({ text: 'hi' })).resolves.toEqual({ id: 7 });
  });

  it('truncates notification text', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await result.current.notifications.pushNotification({ text: 'x'.repeat(500) });

    expect(scheduleNotification.mock.calls[0]?.[0]).toMatchObject({ text: 'x'.repeat(200) });
  });

  it('resolves when cancelling an unknown id', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await expect(result.current.notifications.cancelNotification(999)).resolves.toBeUndefined();
  });
});

describe('theme callback', () => {
  it('emits the picked theme name and current variant before any change', async () => {
    // `themeUseCase.watchTheme` is a passthrough over this resource, so the case is
    // stated on the resource itself instead of on the domain module around it.
    themeResource.instead(() => of({ preference: 'dark', variant: 'dark', name: 'tokyo' } as const));
    const result = { current: createPassiveCallbacks(navigateMock) };

    for await (const item of result.current.theme.subscribeTheme()) {
      expect(item.isOk() && item.value).toEqual({ name: { tag: 'Custom', value: 'tokyo' }, variant: 'Dark' });
      break;
    }
  });
});

describe('preimage callback', () => {
  it('emits a miss immediately when nothing is cached', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    for await (const item of result.current.preimage.lookupPreimage(new Uint8Array([1]))) {
      expect(item.isOk() && item.value).toBeUndefined();
      break;
    }
  });
});

describe('locale callback', () => {
  it('emits the current locale as a BCP 47 tag', async () => {
    saveLocale('ja');

    const result = { current: createPassiveCallbacks(navigateMock) };

    for await (const item of result.current.locale.subscribeLocale()) {
      expect(item.isOk() && item.value).toEqual({ languageTag: 'ja' });
      break;
    }
  });
});

describe('permission status callback', () => {
  beforeEach(() => {
    getSystemDevicePermissionStatus.mockReset();
  });

  afterEach(() => {
    Object.assign(window, { App });
  });

  it('reports the OS status for an OS-gated device permission', async () => {
    getSystemDevicePermissionStatus.mockResolvedValue('denied');

    const result = { current: createPassiveCallbacks(navigateMock) };

    await expect(result.current.permissionStatus!.devicePermissionStatus('Camera')).resolves.toBe('Denied');
    expect(getSystemDevicePermissionStatus).toHaveBeenCalledWith('Camera');
  });

  it('does not ask the OS about a permission it does not gate', async () => {
    const result = { current: createPassiveCallbacks(navigateMock) };

    await expect(result.current.permissionStatus!.devicePermissionStatus('Notifications')).resolves.toBe('NotApplicable');
    expect(getSystemDevicePermissionStatus).not.toHaveBeenCalled();
  });

  it('reports not-applicable when the bridge is unavailable', async () => {
    Object.assign(window, { App: undefined });

    const result = { current: createPassiveCallbacks(navigateMock) };

    await expect(result.current.permissionStatus!.devicePermissionStatus('Camera')).resolves.toBe('NotApplicable');
    expect(getSystemDevicePermissionStatus).not.toHaveBeenCalled();
  });
});
