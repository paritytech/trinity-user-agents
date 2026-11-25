import { type WebviewTag } from 'electron';
import { describe, expect, it, vi } from 'vitest';

import { createWebviewProvider } from './webviewProvider';

function createFakeWebview() {
  const listeners: Record<string, ((event: unknown) => void)[]> = {};
  let resolveExec: () => void = () => {};

  const postMessage = vi.fn();
  const executeJavaScript = vi.fn(
    () =>
      new Promise<void>(resolve => {
        resolveExec = resolve;
      }),
  );

  const webview = {
    addEventListener(event: string, handler: (event: unknown) => void) {
      (listeners[event] ??= []).push(handler);
    },
    removeEventListener(event: string, handler: (event: unknown) => void) {
      listeners[event] = (listeners[event] ?? []).filter(existing => existing !== handler);
    },
    executeJavaScript,
    openDevTools: vi.fn(),
    contentWindow: { postMessage },
  };

  return {
    // A minimal stand-in; the provider touches only these members of the webview.
    // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test double for Electron's WebviewTag, which has no constructible form
    webview: webview as unknown as WebviewTag,
    emit(event: string, payload?: unknown) {
      for (const handler of listeners[event] ?? []) handler(payload);
    },
    resolveExec: () => resolveExec(),
    executeJavaScript,
    postMessage,
    hasListener: (event: string) => (listeners[event] ?? []).length > 0,
  };
}

describe('createWebviewProvider', () => {
  it('transfers the port only after the guest listener script has run', async () => {
    const fake = createFakeWebview();
    createWebviewProvider({ webview: fake.webview });

    fake.emit('dom-ready');

    // The listener-install script is injected, but the port must not be transferred
    // until it resolves — otherwise the guest can miss the port it never listened for.
    expect(fake.executeJavaScript).toHaveBeenCalledTimes(1);
    expect(fake.postMessage).not.toHaveBeenCalled();

    fake.resolveExec();

    await vi.waitFor(() => expect(fake.postMessage).toHaveBeenCalledTimes(1));
  });

  // `pipeProviders` disposes the paired core provider through `subscribeClose`, and
  // reaches for it optionally — so a provider that does not implement it takes the whole
  // close leg down silently, and a guest that goes away leaves its core peer alive.
  it('reports a close to subscribers when disposed', () => {
    const fake = createFakeWebview();
    const provider = createWebviewProvider({ webview: fake.webview });
    const onClose = vi.fn();

    provider.subscribeClose?.(onClose);
    expect(onClose).not.toHaveBeenCalled();

    provider.dispose();

    expect(onClose).toHaveBeenCalledOnce();
  });

  it('reports a close when the guest fails to load', () => {
    const fake = createFakeWebview();
    const provider = createWebviewProvider({ webview: fake.webview });
    const onClose = vi.fn();
    provider.subscribeClose?.(onClose);

    fake.emit('did-fail-load', { errorDescription: 'ERR_FAILED' });

    expect(onClose).toHaveBeenCalledOnce();
    expect(onClose.mock.calls[0]?.[0]).toBeInstanceOf(Error);
  });

  // The contract: a registration after the provider has already closed fires straight
  // away, so a late subscriber cannot wait forever on an event that already happened.
  it('reports the close immediately to a subscriber that arrives after it', () => {
    const fake = createFakeWebview();
    const provider = createWebviewProvider({ webview: fake.webview });
    provider.dispose();

    const onClose = vi.fn();
    provider.subscribeClose?.(onClose);

    expect(onClose).toHaveBeenCalledOnce();
  });

  // A navigation is not a close. The provider survives it and rebinds a fresh channel,
  // so firing here would tear down a pipe that is about to be reused.
  it('does not report a close when the guest merely navigates', async () => {
    const fake = createFakeWebview();
    const provider = createWebviewProvider({ webview: fake.webview });
    const onClose = vi.fn();
    provider.subscribeClose?.(onClose);

    fake.emit('dom-ready');
    fake.resolveExec();
    await vi.waitFor(() => expect(fake.postMessage).toHaveBeenCalledTimes(1));

    expect(onClose).not.toHaveBeenCalled();
  });

  it('detaches its webview listeners on dispose', () => {
    const fake = createFakeWebview();
    const provider = createWebviewProvider({ webview: fake.webview });

    expect(fake.hasListener('dom-ready')).toBe(true);
    expect(fake.hasListener('did-fail-load')).toBe(true);

    provider.dispose();

    expect(fake.hasListener('dom-ready')).toBe(false);
    expect(fake.hasListener('did-fail-load')).toBe(false);
  });
});
