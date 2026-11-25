// @vitest-environment happy-dom

import { type AuthState } from '@parity/truapi-host';
import { act, cleanup, render, waitFor } from '@testing-library/react';
import React, { type PropsWithChildren } from 'react';
import { Subject, of } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { type createWebviewProvider } from '@/shared/truapi';
import { type HexString } from '@/shared/types';
import {
  type PersistedProduct,
  type Product,
  EXECUTABLE_KINDS,
  chainResolveResource,
  dotNsTldResource,
  executableArchiveResource,
  grantTransientDevicePermission,
  productsResource,
} from '@/domains/product';
/* eslint-disable boundaries/dependencies -- the test observes the write where it lands; the barrel exposes no read for it */
import {
  _resetTransientDevicePermissionGrants,
  getTransientDevicePermissionGranted,
} from '@/domains/product/permissions/resource';
/* eslint-enable boundaries/dependencies */
import { productLoading } from '@/aggregates/product-loading';
import { type TrUApiProductProvider, truapiRuntime, truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';
import { type WebviewCrashInfo, type WebviewHealthEntry, type WebviewUnresponsiveInfo } from '@/aggregates/webview-registry';
// eslint-disable-next-line local-rules/no-relative-import-from-root -- cross-target drift guard: pin the renderer's partition string to the main-process builder
import { buildSandboxPartition } from '../../../main/sandbox/lib';

const TEST_HASH: HexString = '0xdeadbeef';

import { Webview } from './Webview';
import { type MockWebviewTag, createMockWebviewTag } from './__tests__/MockWebviewTag';

const {
  providerDispose,
  createWebviewProviderMock,
  createCoreProviderMock,
  unpipeMock,
  pipeProvidersMock,
  registryRegister,
  registryUnregister,
  registryClearCrash,
  registryClearUnresponsive,
  registryClearHealth,
  useWebviewCrashMock,
  useWebviewUnresponsiveMock,
  useWebviewHealthMock,
} = vi.hoisted(() => {
  const providerDispose = vi.fn();
  const coreProviderDispose = vi.fn();
  const unpipeMock = vi.fn();
  return {
    providerDispose,

    createWebviewProviderMock: vi.fn(() => ({ dispose: providerDispose }) as unknown as ReturnType<typeof createWebviewProvider>),
    coreProviderDispose,
    // Opens by default: the guest mounts only once its core provider exists, so every
    // case that renders a guest needs one. A case about the not-yet-open window opts out
    // with mockImplementationOnce.
    createCoreProviderMock: vi.fn((_product: unknown) =>
      Promise.resolve({ dispose: coreProviderDispose } as unknown as TrUApiProductProvider),
    ),
    unpipeMock,
    pipeProvidersMock: vi.fn((_product: unknown, _core: unknown) => unpipeMock),
    registryRegister: vi.fn(),
    registryUnregister: vi.fn(),
    registryClearCrash: vi.fn(),
    registryClearUnresponsive: vi.fn(),
    registryClearHealth: vi.fn(),

    useWebviewCrashMock: vi.fn<() => WebviewCrashInfo | null>(() => null),
    useWebviewUnresponsiveMock: vi.fn<() => WebviewUnresponsiveInfo | null>(() => null),
    useWebviewHealthMock: vi.fn<() => WebviewHealthEntry | null>(() => null),
  };
});

// The core provider needs a live runtime, so the factory is spied in place; auth itself is
// published through the real runtime, and `useTruapiAuthState` reads it for real.
vi.spyOn(truapiRuntimeUseCase, 'createProvider').mockImplementation(p => createCoreProviderMock(p));
// The host keys the guest reload on the auth tag alone, so the fixture carries only that.
const publishAuth = (tag: AuthState['tag']) => act(() => truapiRuntimeUseCase.publishAuthState({ tag } as AuthState));
const productLoadingSet = vi.spyOn(productLoading, 'set');

vi.mock(import('@/shared/truapi'), async importOriginal => {
  const actual = await importOriginal();

  return {
    ...actual,
    createWebviewProvider: () => createWebviewProviderMock(),

    pipeProviders: (a: unknown, b: unknown) => pipeProvidersMock(a, b),
  };
});

vi.mock(import('@/aggregates/webview-registry'), async importOriginal => {
  const actual = await importOriginal();

  return {
    ...actual,
    webviewRegistry: {
      ...actual.webviewRegistry,
      register: registryRegister,
      unregister: registryUnregister,
      clearCrash: registryClearCrash,
      clearUnresponsive: registryClearUnresponsive,
      clearHealth: registryClearHealth,
    },
    useWebviewCrash: () => useWebviewCrashMock(),

    useWebviewUnresponsive: () => useWebviewUnresponsiveMock(),

    useWebviewHealth: () => useWebviewHealthMock(),
  };
});

const Providers = ({ children }: PropsWithChildren) => <TranslationProvider>{children}</TranslationProvider>;

let mockTag: MockWebviewTag;
let createElementSpy: ReturnType<typeof vi.spyOn>;

function resolvedArchive(origin: string) {
  return { contenthash: TEST_HASH, archive: { domain: origin.replace('polkadot://', ''), origin, files: {} } };
}

function productRecord(baseName: string) {
  return {
    baseName,
    displayName: 'App',
    description: '',
    icon: { cid: '', format: 'png' },
    executables: Object.fromEntries(
      EXECUTABLE_KINDS.map(kind => [kind, { kind, identifier: baseName, appVersion: [0, 0, 0], contenthash: TEST_HASH }]),
    ),
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;
}

function seedProducts(...baseNames: string[]) {
  productsResource.instead(() => of(baseNames.map(productRecord)));
}

const settleProducts = () => waitFor(() => expect(productsResource.snapshot().length).toBeGreaterThan(0));

// Both reads settle a tick after mount, which is the timing every `.dot` case
// below has to survive.
function seedArchive(origin: string) {
  executableArchiveResource.instead(() => resolvedArchive(origin));
}

function srcOf(container: HTMLElement) {
  return container.querySelector('[data-testid="webview-host"]')?.getAttribute('src');
}

// The host element only mounts once the product is `ready`, which for a `.dot`
// identifier means both reads have landed.
// The events the host bound on the guest tag, so a failure names what was attached
// instead of reporting `false`.
function boundEvents() {
  return mockTag.addEventListener.mock.calls.map(([event]) => event);
}

function mounted(container: HTMLElement) {
  return waitFor(() => expect(container.querySelector('[data-testid="webview-host"]')).not.toBeNull());
}

beforeEach(() => {
  vi.clearAllMocks();
  truapiRuntimeUseCase.publishAuthState({ tag: 'Disconnected' });
  mockTag = createMockWebviewTag();
  seedProducts('app.dot');
  // Replace document.createElement('webview') so React's reconciler installs our mock.
  const realCreateElement = document.createElement.bind(document);
  createElementSpy = vi
    .spyOn(document, 'createElement')

    .mockImplementation(((name: string, options?: ElementCreationOptions) => {
      if (name === 'webview') return mockTag as unknown as HTMLElement;
      return realCreateElement(name, options);
    }) as typeof document.createElement) as ReturnType<typeof vi.spyOn>;
});

afterEach(() => {
  cleanup();
  createElementSpy?.mockRestore();
  truapiRuntimeUseCase.dispose();
  _resetTransientDevicePermissionGrants();
});

// The TLD read starts in an effect and settles a tick after mount. Navigation
// decisions treat an unsettled TLD as "not a dotNS identifier", so every case that
// depends on it flushes the read first. The wait is on the resource's own cache,
// which the global `afterEach` empties between tests.
async function settleTld() {
  await waitFor(() => expect(Object.keys(dotNsTldResource.snapshot()).length).toBeGreaterThan(0));
}

// A localhost product needs neither read, but `useDisplayedProduct` still holds the
// products subscription, so both settle a tick after mount either way.
async function localhostMounted() {
  const result = render(
    <Providers>
      <Webview kind="app" identifier="http://localhost:5173" visible={true} />
    </Providers>,
  );
  await settleProducts();
  // The TLD read is the last of the three to land.
  await settleTld();

  return result;
}

describe('Webview — lifecycle', () => {
  it('sets productLoading=true on mount and =false on unmount', async () => {
    const { unmount } = await localhostMounted();
    expect(productLoadingSet).toHaveBeenCalledWith('http://localhost:5173', true);
    productLoadingSet.mockClear();
    unmount();
    expect(productLoadingSet).toHaveBeenCalledWith('http://localhost:5173', false);
  });

  it('registers webContentsId on dom-ready and unregisters on unmount', async () => {
    const { unmount } = await localhostMounted();
    act(() => mockTag.dispatch('dom-ready', {}));
    expect(registryRegister).toHaveBeenCalledWith('http://localhost:5173', 42);
    unmount();
    expect(registryUnregister).toHaveBeenCalledWith('http://localhost:5173');
  });

  it('clears productLoading + webviewLoading on did-finish-load', async () => {
    await localhostMounted();
    productLoadingSet.mockClear();
    act(() => mockTag.dispatch('did-finish-load', {}));
    expect(productLoadingSet).toHaveBeenCalledWith('http://localhost:5173', false);
  });

  // The guest hands its channel over on its own `dom-ready`. A guest that loaded before
  // its core provider existed (a cached archive loads faster than the stored session is
  // restored) would run with no wire and queue calls that nothing answers.
  it('mounts the guest only once its core provider is open', async () => {
    let openCore!: (provider: TrUApiProductProvider) => void;
    createCoreProviderMock.mockImplementationOnce(
      (_product: unknown) => new Promise<TrUApiProductProvider>(resolve => (openCore = resolve)),
    );
    seedArchive('polkadot://app.dot');
    const { container } = render(<Webview kind="app" identifier="app.dot" visible={true} />, { wrapper: Providers });
    await waitFor(() => expect(createCoreProviderMock).toHaveBeenCalled());
    await settleTld();
    expect(container.querySelector('[data-testid="webview-host"]')).toBeNull();

    act(() => openCore({ dispose: vi.fn() } as unknown as TrUApiProductProvider));

    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
  });

  // Opening a product commits it: the commit drops its chain-resolve entry, and the live
  // DB row lands a tick later. Reading that gap as "no such product" tore down the guest
  // that had just loaded and mounted a fresh one.
  it('keeps the guest when the product it shows is committed', async () => {
    const products = new Subject<PersistedProduct[]>();
    productsResource.instead(() => products);
    chainResolveResource.instead(() => productRecord('new.dot') as Product);
    seedArchive('polkadot://app.new.dot');
    const { container } = render(<Webview kind="app" identifier="new.dot" visible={true} />, { wrapper: Providers });
    act(() => products.next([]));
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.new.dot/'));
    const guestsCreated = () => createElementSpy.mock.calls.filter((call: unknown[]) => call[0] === 'webview').length;
    expect(guestsCreated()).toBe(1);

    act(() => chainResolveResource.invalidateAll());
    act(() => products.next([productRecord('new.dot')]));

    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.new.dot/'));
    expect(guestsCreated()).toBe(1);
  });

  it('pipes the product to a core provider and tears both down on unmount', async () => {
    createCoreProviderMock.mockImplementationOnce((_product: unknown) =>
      Promise.resolve({ dispose: vi.fn() } as unknown as TrUApiProductProvider),
    );
    const { unmount } = await localhostMounted();

    await waitFor(() => expect(pipeProvidersMock).toHaveBeenCalled());
    expect(createCoreProviderMock).toHaveBeenCalledWith({ productId: 'http://localhost:5173', executionKind: 'App' });

    unmount();

    expect(unpipeMock).toHaveBeenCalled();
    expect(providerDispose).toHaveBeenCalled();
  });

  it('sets productLoading=false when executable resolution fails', async () => {
    executableArchiveResource.instead(() => Promise.reject(new Error('dns')));
    render(
      <Providers>
        <Webview kind="app" identifier="app.dot" visible={true} />
      </Providers>,
    );
    await waitFor(() => expect(productLoadingSet).toHaveBeenCalledWith('app.dot', false));
  });
});

describe('Webview — src derivation and reload', () => {
  it('derives src from normalizeLocalhostUrl for localhost identifier', async () => {
    const { container } = await localhostMounted();
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.getAttribute('src')).toMatch(/^http:\/\/localhost/);
  });

  it('derives src from archive.origin for resolved .dot identifier', async () => {
    seedArchive('polkadot://app.app.dot');
    const { container } = render(<Webview kind="app" identifier="app.dot" pathname="/x" visible={true} />, {
      wrapper: Providers,
    });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.app.dot/x'));
  });

  it('does not update src when pathname matches lastWebviewPathnameRef', async () => {
    seedArchive('polkadot://app.dot');
    const { container, rerender } = render(<Webview kind="app" identifier="app.dot" pathname="a" visible={true} />, {
      wrapper: Providers,
    });
    await settleTld();
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/a'));
    const initialSrc = srcOf(container);
    // Synthesize a navigation-in-page event so lastWebviewPathnameRef updates to 'b'.
    act(() => mockTag.dispatch('did-navigate-in-page', { url: 'polkadot://app.dot/b', isMainFrame: true }));
    rerender(<Webview kind="app" identifier="app.dot" pathname="b" visible={true} />);
    const finalSrc = srcOf(container);
    // lastWebviewPathnameRef now holds 'b', matching the pathname prop 'b'.
    // The guard in the src-derivation effect fires correctly — src does NOT update.
    expect(finalSrc).toBe(initialSrc);
    // ...and the guest is not re-loaded either: it is already where the host wants it.
    expect(mockTag.loadURL).not.toHaveBeenCalled();
  });

  it('loads the guest imperatively when the host navigates back to the path it last set', async () => {
    seedArchive('polkadot://app.dot');
    const { container, rerender } = render(<Webview kind="app" identifier="app.dot" pathname="" visible={true} />, {
      wrapper: Providers,
    });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    // The guest moves itself (link click inside the product); the host follows with the pathname.
    act(() => mockTag.dispatch('did-navigate-in-page', { url: 'polkadot://app.dot/b', isMainFrame: true }));
    rerender(<Webview kind="app" identifier="app.dot" pathname="b" visible={true} />);
    // Host Back to the root — the value `src` already holds, so writing it moves nothing.
    rerender(<Webview kind="app" identifier="app.dot" pathname="" visible={true} />);
    expect(mockTag.loadURL).toHaveBeenCalledWith('polkadot://app.dot/');
  });

  it('keeps following host navigation after an imperative load', async () => {
    seedArchive('polkadot://app.dot');
    const { container, rerender } = render(<Webview kind="app" identifier="app.dot" pathname="" visible={true} />, {
      wrapper: Providers,
    });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    act(() => mockTag.dispatch('did-navigate-in-page', { url: 'polkadot://app.dot/b', isMainFrame: true }));
    rerender(<Webview kind="app" identifier="app.dot" pathname="b" visible={true} />);
    rerender(<Webview kind="app" identifier="app.dot" pathname="" visible={true} />);
    mockTag.loadURL.mockClear();
    // Forward to 'b'. The host drove the guest to the root, so 'b' is a real move again.
    rerender(<Webview kind="app" identifier="app.dot" pathname="b" visible={true} />);
    expect(mockTag.loadURL).toHaveBeenCalledWith('polkadot://app.dot/b');
  });

  it('triggers webviewRef.reload() when auth changes while ready', async () => {
    seedArchive('polkadot://app.dot');
    // Use a wrapper that can force re-renders of the memoized Webview via its own state.
    // memo() only skips re-renders when triggered by a parent re-render with unchanged props;
    const { container } = render(<Webview kind="app" identifier="app.dot" visible={true} onPathnameChange={() => {}} />, {
      wrapper: Providers,
    });
    // `ready` gates the reload, and for a `.dot` product that means the archive landed.
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    // The guest has loaded (dom-ready) — this is a genuine post-load session change (login
    // while the product is open), so reload() is a valid call and must fire.
    act(() => mockTag.dispatch('dom-ready', {}));
    expect(mockTag.reload).not.toHaveBeenCalled();
    publishAuth('Connected');
    expect(mockTag.reload).toHaveBeenCalledTimes(1);
  });

  // The renderer boots with no auth state until the core's worker first answers. A product
  // restored with its tab can be loaded by then; its first report is not a session change,
  // and reloading there wipes whatever the product was doing.
  it('does not reload a loaded product when the core first reports auth', async () => {
    act(() => truapiRuntime.set(prev => ({ ...prev, authState: null })));
    seedArchive('polkadot://app.dot');
    const { container } = render(<Webview kind="app" identifier="app.dot" visible={true} />, { wrapper: Providers });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    act(() => mockTag.dispatch('dom-ready', {}));

    publishAuth('Connected');
    expect(mockTag.reload).not.toHaveBeenCalled();

    publishAuth('Disconnected');
    expect(mockTag.reload).toHaveBeenCalledTimes(1);
  });

  it('does not crash when auth changes before the webview attaches (localhost hard-reload race)', async () => {
    // A localhost product is `ready` synchronously, so a session change can land after the
    // guest element mounts but before it has emitted dom-ready. reload() throws in that
    // window; the host must swallow it rather than surface the error-boundary fallback.
    // Regression for the Cmd+Shift+R crash.
    const { container } = render(
      <Webview kind="app" identifier="http://localhost:5173" visible={true} onPathnameChange={() => {}} />,
      { wrapper: Providers },
    );
    await mounted(container);
    // No dom-ready dispatched — the guest is not attached yet.
    expect(() => publishAuth('Connected')).not.toThrow();
    expect(mockTag.reload).toHaveBeenCalled();
  });

  it('does not reload when auth changes while not ready', async () => {
    executableArchiveResource.instead(() => new Promise(() => {}));
    await act(async () => {
      render(<Webview kind="app" identifier="app.dot" visible={true} />, { wrapper: Providers });
    });
    publishAuth('Connected');
    expect(mockTag.reload).not.toHaveBeenCalled();
  });

  it('reloadTrigger$ emits → loadURL(src) called', async () => {
    const reload$ = new Subject<void>();
    seedArchive('polkadot://app.dot');
    const { container } = render(<Webview kind="app" identifier="app.dot" reloadTrigger$={reload$} visible={true} />, {
      wrapper: Providers,
    });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    act(() => reload$.next());
    expect(mockTag.loadURL).toHaveBeenCalledWith('polkadot://app.dot/');
  });

  it('reloadTrigger$ reloads the page the guest is on, not the one it was mounted with', async () => {
    const reload$ = new Subject<void>();
    seedArchive('polkadot://app.dot');
    const { container, rerender } = render(
      <Webview kind="app" identifier="app.dot" pathname="" reloadTrigger$={reload$} visible={true} />,
      { wrapper: Providers },
    );
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    act(() => mockTag.dispatch('did-navigate-in-page', { url: 'polkadot://app.dot/b', isMainFrame: true }));
    rerender(<Webview kind="app" identifier="app.dot" pathname="b" reloadTrigger$={reload$} visible={true} />);
    mockTag.loadURL.mockClear();
    act(() => reload$.next());
    expect(mockTag.loadURL).toHaveBeenCalledWith('polkadot://app.dot/b');
  });

  it("ends a product's one-shot device grants when it unmounts", async () => {
    const camera = { productId: 'app.dot', permission: 'Camera', executionKind: 'App' } as const;
    seedArchive('polkadot://app.dot');
    grantTransientDevicePermission(camera);
    const { container, unmount } = render(<Webview kind="app" identifier="app.dot" visible={true} />, { wrapper: Providers });
    await waitFor(() => expect(srcOf(container)).toBe('polkadot://app.dot/'));
    expect(getTransientDevicePermissionGranted(camera)).toBe(true);

    unmount();
    expect(getTransientDevicePermissionGranted(camera)).toBe(false);
  });

  it('reloadTrigger$ unsubscribes on unmount', () => {
    const reload$ = new Subject<void>();
    seedArchive('polkadot://app.dot');
    const { unmount } = render(<Webview kind="app" identifier="app.dot" reloadTrigger$={reload$} visible={true} />, {
      wrapper: Providers,
    });
    unmount();
    expect(reload$.observed).toBe(false);
  });
});

describe('Webview — navigation dispatcher integration', () => {
  async function dotMounted(props: { onCrossProductLink?: (t: unknown) => void; onPathnameChange?: (p: string) => void } = {}) {
    seedArchive('polkadot://app.dot');
    const result = render(<Webview kind="app" identifier="app.dot" visible={true} {...props} />, { wrapper: Providers });
    await settleTld();
    await waitFor(() => expect(srcOf(result.container)).toBe('polkadot://app.dot/'));

    return result;
  }

  it('preventDefault + stop + emit on cross-product will-navigate', async () => {
    const onCross = vi.fn();
    await dotMounted({ onCrossProductLink: onCross });
    const e = { url: 'polkadot://other.dot/x', preventDefault: vi.fn() };
    act(() => mockTag.dispatch('will-navigate', e));
    expect(e.preventDefault).toHaveBeenCalled();
    expect(mockTag.stop).toHaveBeenCalled();
    expect(onCross).toHaveBeenCalledWith(expect.objectContaining({ identifier: 'other.dot' }));
  });

  it('emits onPathnameChange on same-product polkadot will-navigate (no preventDefault)', async () => {
    const onPath = vi.fn();
    await dotMounted({ onPathnameChange: onPath });
    const e = { url: 'polkadot://app.dot/sub', preventDefault: vi.fn() };
    act(() => mockTag.dispatch('will-navigate', e));
    expect(onPath).toHaveBeenCalledWith('sub');
    expect(e.preventDefault).not.toHaveBeenCalled();
  });

  it('did-navigate fallback calls loadURL(desired) on race-loss', async () => {
    await dotMounted();
    act(() => mockTag.dispatch('did-navigate', { url: 'polkadot://other.dot/x' }));
    expect(mockTag.loadURL).toHaveBeenCalledWith('polkadot://app.dot/');
  });

  it('did-navigate-in-page emits pathname change for same-product', async () => {
    const onPath = vi.fn();
    await dotMounted({ onPathnameChange: onPath });
    act(() => mockTag.dispatch('did-navigate-in-page', { url: 'polkadot://app.dot/spa', isMainFrame: true }));
    expect(onPath).toHaveBeenCalledWith('spa');
  });
});

describe('Webview — console-message routing', () => {
  it('attaches console-message listener for localhost identifier', async () => {
    await localhostMounted();
    expect(boundEvents()).toContain('console-message');
  });

  // Regression guard: the listener is attached conditionally on the TLD, which
  // settles after the element mounts. It only lands because `resolvedTld` is a
  // dependency of the effect that attaches it.
  it('attaches console-message listener for .dot identifier', async () => {
    seedArchive('polkadot://app.dot');
    render(<Webview kind="app" identifier="app.dot" visible={true} />, { wrapper: Providers });
    await settleTld();
    expect(boundEvents()).toContain('console-message');
  });

  it('does NOT attach console-message listener for non-dot, non-localhost identifier', async () => {
    await act(async () => {
      render(<Webview kind="app" identifier="not-a-dot-identifier" visible={true} />, { wrapper: Providers });
    });
    expect(boundEvents()).not.toContain('console-message');
  });

  it('routes levels 0/1/2/3 to console.debug/info/warn/error and unknown to info', async () => {
    const debug = vi.spyOn(console, 'debug').mockImplementation(() => {});
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    await localhostMounted();
    // Filter out the component's own state-transition / event logs (prefixed with `[Webview:`)
    // — only count the console-message routing calls.
    const isStateLog = (call: unknown[]) => typeof call[0] === 'string' && call[0].startsWith('[Webview:');
    debug.mockClear();
    info.mockClear();
    warn.mockClear();
    error.mockClear();
    for (const level of [0, 1, 2, 3, 99]) {
      act(() => mockTag.dispatch('console-message', { level, message: 'm' }));
    }
    const consoleMessageDebug = debug.mock.calls.filter(c => !isStateLog(c));
    const consoleMessageInfo = info.mock.calls.filter(c => !isStateLog(c));
    const consoleMessageWarn = warn.mock.calls.filter(c => !isStateLog(c));
    const consoleMessageError = error.mock.calls.filter(c => !isStateLog(c));
    expect(consoleMessageDebug).toHaveLength(1); // level 0
    expect(consoleMessageInfo).toHaveLength(2); // level 1 + fallback (level 99)
    expect(consoleMessageWarn).toHaveLength(1);
    expect(consoleMessageError).toHaveLength(1);
    debug.mockRestore();
    info.mockRestore();
    warn.mockRestore();
    error.mockRestore();
  });
});

describe('Webview — crash overlay', () => {
  const fakeCrash = { webContentsId: 7, url: 'polkadot://app.dot/', reason: 'oom', exitCode: 5, at: 1700000000000 };

  it('shows CrashOverlay when useWebviewCrash returns a crash', async () => {
    useWebviewCrashMock.mockReturnValue(fakeCrash);
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="crash-overlay-reload"]')).not.toBeNull();
  });

  it('does not show CrashOverlay when there is no crash', async () => {
    useWebviewCrashMock.mockReturnValue(null);
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="crash-overlay-reload"]')).toBeNull();
  });

  it('calls clearCrash and webview.reload on Reload click', async () => {
    useWebviewCrashMock.mockReturnValue(fakeCrash);
    const { container } = await localhostMounted();
    act(() => mockTag.dispatch('dom-ready', {}));
    const btn = container.querySelector('[data-testid="crash-overlay-reload"]');
    act(() => (btn as HTMLElement).click());
    expect(registryClearCrash).toHaveBeenCalledWith(7);
    expect(mockTag.reload).toHaveBeenCalled();
  });
});

describe('Webview — unresponsive overlay', () => {
  const fakeUnresponsive = { webContentsId: 11, url: 'polkadot://app.dot/', at: 1700000000000 };

  // beforeEach only clears call history; mockReturnValue from earlier suites persists,
  // so we restore the default no-crash/no-unresponsive baseline explicitly.
  beforeEach(() => {
    useWebviewCrashMock.mockReturnValue(null);
    useWebviewUnresponsiveMock.mockReturnValue(null);
  });

  it('shows UnresponsiveOverlay and applies blur+grayscale to the webview', async () => {
    useWebviewUnresponsiveMock.mockReturnValue(fakeUnresponsive);
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="unresponsive-overlay-reload"]')).not.toBeNull();
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.className).toMatch(/blur-sm/);
    expect(tag?.className).toMatch(/grayscale/);
  });

  it('does not show UnresponsiveOverlay when there is no unresponsive entry', async () => {
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="unresponsive-overlay-reload"]')).toBeNull();
  });

  it('crash overlay takes precedence and the webview is not double-styled', async () => {
    useWebviewCrashMock.mockReturnValue({
      webContentsId: 11,
      url: 'polkadot://app.dot/',
      reason: 'oom',
      exitCode: 5,
      at: 1700000000000,
    });
    useWebviewUnresponsiveMock.mockReturnValue(fakeUnresponsive);
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="crash-overlay-reload"]')).not.toBeNull();
    expect(container.querySelector('[data-testid="unresponsive-overlay-reload"]')).toBeNull();
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.className).not.toMatch(/blur-sm/);
  });

  it('reload click clears unresponsive state and reloads the webview', async () => {
    useWebviewUnresponsiveMock.mockReturnValue(fakeUnresponsive);
    const { container } = await localhostMounted();
    act(() => mockTag.dispatch('dom-ready', {}));
    const btn = container.querySelector('[data-testid="unresponsive-overlay-reload"]');
    act(() => (btn as HTMLElement).click());
    expect(registryClearUnresponsive).toHaveBeenCalledWith(11);
    expect(mockTag.reload).toHaveBeenCalled();
  });
});

describe('Webview — degraded banner', () => {
  beforeEach(() => {
    useWebviewCrashMock.mockReturnValue(null);
    useWebviewUnresponsiveMock.mockReturnValue(null);
    useWebviewHealthMock.mockReturnValue(null);
  });

  it('shows DegradedBanner when useWebviewHealth returns a degraded entry', async () => {
    useWebviewHealthMock.mockReturnValue({
      webContentsId: 42,
      productId: 'p1',
      state: 'degraded',
      reason: { kind: 'heartbeat-rtt-high' },
      since: Date.now(),
    });
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="degraded-banner-reload"]')).not.toBeNull();
  });

  it('does not show DegradedBanner when CrashOverlay is showing', async () => {
    useWebviewCrashMock.mockReturnValue({
      webContentsId: 42,
      url: 'about:blank',
      reason: 'crashed',
      exitCode: 0,
      at: Date.now(),
    });
    useWebviewHealthMock.mockReturnValue({
      webContentsId: 42,
      productId: 'p1',
      state: 'degraded',
      reason: { kind: 'heartbeat-rtt-high' },
      since: Date.now(),
    });
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="degraded-banner"]')).toBeNull();
  });

  it('does not show DegradedBanner when UnresponsiveOverlay is showing', async () => {
    useWebviewUnresponsiveMock.mockReturnValue({
      webContentsId: 42,
      url: 'about:blank',
      at: Date.now(),
    });
    useWebviewHealthMock.mockReturnValue({
      webContentsId: 42,
      productId: 'p1',
      state: 'degraded',
      reason: { kind: 'heartbeat-rtt-high' },
      since: Date.now(),
    });
    const { container } = await localhostMounted();
    expect(container.querySelector('[data-testid="degraded-banner"]')).toBeNull();
  });

  it('reload click clears health entry and reloads webview', async () => {
    useWebviewHealthMock.mockReturnValue({
      webContentsId: 42,
      productId: 'p1',
      state: 'degraded',
      reason: { kind: 'heartbeat-rtt-high' },
      since: Date.now(),
    });
    const { container } = await localhostMounted();
    act(() => mockTag.dispatch('dom-ready', {}));
    const btn = container.querySelector('[data-testid="degraded-banner-reload"]') as HTMLButtonElement;
    act(() => (btn as HTMLElement).click());
    expect(registryClearHealth).toHaveBeenCalledWith(42);
    expect(mockTag.reload).toHaveBeenCalled();
  });
});

describe('Webview — visibility beacon', () => {
  let sendWebviewVisibility: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    sendWebviewVisibility = vi.fn();
    // @ts-expect-error stub
    globalThis.window.App = { sendWebviewVisibility };
  });

  afterEach(() => {
    // @ts-expect-error stub
    delete globalThis.window.App;
  });

  it('does not send visible=true beacon when visible prop is false', async () => {
    render(
      <Providers>
        <Webview kind="app" identifier="http://localhost:5173" visible={false} />
      </Providers>,
    );
    await settleProducts();
    await settleTld();
    act(() => mockTag.dispatch('dom-ready', {}));
    const calls = sendWebviewVisibility.mock.calls;
    expect(calls.length).toBeGreaterThan(0);
    expect(calls.every(([, v]) => v === false)).toBe(true);
  });

  it('sends visible=true beacon when visible prop is true (default)', async () => {
    await localhostMounted();
    act(() => mockTag.dispatch('dom-ready', {}));
    expect(sendWebviewVisibility.mock.calls.map(([, visible]) => visible)).toContain(true);
  });
});

describe('Webview — partition encoding', () => {
  it('partition is sandbox-app-<encoded id>', async () => {
    const { container } = await localhostMounted();
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.getAttribute('partition')).toBe(`sandbox-app-${encodeURIComponent('http://localhost:5173')}`);
  });

  it('partition is path-traversal safe (encodes slashes)', async () => {
    seedProducts('foo/bar');
    seedArchive('polkadot://x');
    const { container } = render(<Webview kind="app" identifier="foo/bar" visible={true} />, { wrapper: Providers });
    await mounted(container);
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.getAttribute('partition')).toBe('sandbox-app-foo%2Fbar');
  });

  // The renderer cannot import main/ code in production, so Webview hand-builds the
  // partition string. This pins it to the main-process builder for EVERY executable
  // kind — iterating EXECUTABLE_KINDS means a newly added kind is covered automatically
  // and fails the guard the moment the two sides drift.
  it.each(EXECUTABLE_KINDS)('%s partition matches the main-process builder (cross-target drift guard)', async kind => {
    seedProducts('foo.dot');
    seedArchive('polkadot://x');
    const { container } = render(<Webview kind={kind} identifier="foo.dot" visible={true} />, { wrapper: Providers });
    await mounted(container);
    const tag = container.querySelector('[data-testid="webview-host"]');
    expect(tag?.getAttribute('partition')).toBe(buildSandboxPartition('foo.dot', kind));
  });
});

describe('Webview — guest wheel', () => {
  async function mountWith(onGuestWheel?: (wheel: { deltaX: number; deltaY: number; consumed: boolean }) => void) {
    const result = render(
      <Providers>
        <Webview kind="app" identifier="http://localhost:5173" visible={true} onGuestWheel={onGuestWheel} />
      </Providers>,
    );
    await settleProducts();
    await settleTld();
    return result;
  }

  const ipcMessage = (channel: string, ...args: unknown[]) => act(() => mockTag.dispatch('ipc-message', { channel, args }));

  it('forwards host:wheel payloads', async () => {
    const onGuestWheel = vi.fn();
    await mountWith(onGuestWheel);
    ipcMessage('host:wheel', { deltaX: -12, deltaY: 1, consumed: false });
    expect(onGuestWheel).toHaveBeenCalledWith({ deltaX: -12, deltaY: 1, consumed: false });
  });

  it('ignores other channels and malformed payloads', async () => {
    const onGuestWheel = vi.fn();
    await mountWith(onGuestWheel);
    ipcMessage('something-else', { deltaX: -12, deltaY: 1, consumed: false });
    ipcMessage('host:wheel', { deltaX: '-12', deltaY: 1, consumed: false });
    ipcMessage('host:wheel', { deltaX: -12, deltaY: 1 });
    ipcMessage('host:wheel', { deltaX: Number.NaN, deltaY: 1, consumed: false });
    ipcMessage('host:wheel', { deltaX: -12, deltaY: Number.POSITIVE_INFINITY, consumed: false });
    ipcMessage('host:wheel', null);
    ipcMessage('host:wheel');
    expect(onGuestWheel).not.toHaveBeenCalled();
  });

  it('attaches no ipc-message listener without the prop', async () => {
    await mountWith(undefined);
    expect(mockTag.listenerCount('ipc-message')).toBe(0);
  });

  it('removes its listener on unmount', async () => {
    const { unmount } = await mountWith(vi.fn());
    expect(mockTag.listenerCount('ipc-message')).toBe(1);
    unmount();
    expect(mockTag.listenerCount('ipc-message')).toBe(0);
  });
});
