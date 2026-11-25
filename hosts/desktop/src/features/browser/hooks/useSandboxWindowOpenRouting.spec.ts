// @vitest-environment happy-dom

import { renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { openDotNsUrlSideEffect } from '../di';

import { useSandboxWindowOpenRouting } from './useSandboxWindowOpenRouting';

// The TLD comes from `dotNsTldResource`'s own mock (`.dot`), so these cases run the
// real use case and the real parse — only the tab-opening end is observed.
const opened = vi.fn();
const openHandler = { available: () => true, body: opened };

// The hook hands main's unsubscribe back as its cleanup, so capture the handler main
// would have registered and drive it the way the sandbox does.
function mountAndCapture() {
  let handler!: (payload: { url: string }) => Promise<boolean>;
  const unsubscribe = vi.fn();

  vi.stubGlobal('window', {
    ...window,
    App: {
      onSandboxWindowOpen: (cb: typeof handler) => {
        handler = cb;

        return unsubscribe;
      },
    },
  });

  openDotNsUrlSideEffect.registerHandler(openHandler);
  const view = renderHook(() => useSandboxWindowOpenRouting());

  return { handler, unsubscribe, view };
}

afterEach(() => {
  openDotNsUrlSideEffect.removeHandler(openHandler);
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe('useSandboxWindowOpenRouting', () => {
  it('opens a product URL as a tab and reports it handled', async () => {
    const { handler } = mountAndCapture();

    await expect(handler({ url: 'https://calculator.dot' })).resolves.toBe(true);
    expect(opened).toHaveBeenCalledWith({ identifier: 'calculator.dot', pathname: '' });
  });

  // The `.li` gateway form is why this decision cannot be made in the main process.
  it('recognises the .li gateway alias for the active TLD', async () => {
    const { handler } = mountAndCapture();

    await expect(handler({ url: 'https://calculator.dot.li/settings' })).resolves.toBe(true);
    expect(opened).toHaveBeenCalledWith({ identifier: 'calculator.dot', pathname: 'settings' });
  });

  it('leaves a plain web URL to the caller and opens nothing', async () => {
    const { handler } = mountAndCapture();

    await expect(handler({ url: 'https://example.com/docs' })).resolves.toBe(false);
    expect(opened).not.toHaveBeenCalled();
  });

  // A name under another network's TLD must not be claimed as this network's.
  it('does not claim a name outside the active TLD', async () => {
    const { handler } = mountAndCapture();

    await expect(handler({ url: 'https://calculator.paseo' })).resolves.toBe(false);
    expect(opened).not.toHaveBeenCalled();
  });

  it('unregisters the handler on unmount', () => {
    const { unsubscribe, view } = mountAndCapture();

    view.unmount();

    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});
