// @vitest-environment happy-dom

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type CallInitDelivery } from './callInitPort';

const LAUNCH = { direction: 'outgoing', purpose: 'video', offerId: 'offer-1', peerName: 'Peer' };
const ICE_CONFIG = { turnHost: 'turn.example', turnSecret: 'secret' };

// Re-import per test: the listener is registered once at module load.
async function loadModule() {
  vi.resetModules();

  return import('./callInitPort');
}

function deliver(origin: string) {
  const { port1 } = new MessageChannel();
  const event = new MessageEvent('message', {
    data: { __callInit: { launch: LAUNCH, iceConfig: ICE_CONFIG } },
    origin,
    source: window,
    ports: [port1],
  });
  window.dispatchEvent(event);
}

describe('callInitPort', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  // Packaged builds load the renderer over `file://`, where `location.origin`
  // serializes to "file://" but the delivered `event.origin` is the opaque
  // "null". Comparing the two dropped every delivery and left the call window
  // blank.
  it('accepts the delivery when event.origin is the opaque "null" of a file:// document', async () => {
    const { onCallInit } = await loadModule();
    const received = vi.fn<(delivery: CallInitDelivery) => void>();
    onCallInit(received);

    deliver('null');

    expect(received).toHaveBeenCalledTimes(1);
    expect(received.mock.calls[0]?.[0].callInit.launch).toStrictEqual(LAUNCH);
    expect(received.mock.calls[0]?.[0].port).toBeDefined();
  });

  it('accepts the delivery when event.origin matches the document origin', async () => {
    const { onCallInit } = await loadModule();
    const received = vi.fn<(delivery: CallInitDelivery) => void>();
    onCallInit(received);

    deliver(window.location.origin);

    expect(received).toHaveBeenCalledTimes(1);
  });

  it('buffers a delivery that arrives before a subscriber and replays it', async () => {
    const { onCallInit } = await loadModule();

    deliver('null');

    const received = vi.fn<(delivery: CallInitDelivery) => void>();
    onCallInit(received);

    expect(received).toHaveBeenCalledTimes(1);
  });

  it('ignores a message posted by another window', async () => {
    const { onCallInit } = await loadModule();
    const received = vi.fn<(delivery: CallInitDelivery) => void>();
    onCallInit(received);

    const { port1 } = new MessageChannel();
    window.dispatchEvent(
      new MessageEvent('message', {
        data: { __callInit: { launch: LAUNCH, iceConfig: ICE_CONFIG } },
        origin: window.location.origin,
        source: null,
        ports: [port1],
      }),
    );

    expect(received).not.toHaveBeenCalled();
  });

  it('ignores a malformed payload', async () => {
    const { onCallInit } = await loadModule();
    const received = vi.fn<(delivery: CallInitDelivery) => void>();
    onCallInit(received);

    const { port1 } = new MessageChannel();
    window.dispatchEvent(
      new MessageEvent('message', {
        data: { __callInit: { launch: { direction: 'sideways' }, iceConfig: ICE_CONFIG } },
        origin: 'null',
        source: window,
        ports: [port1],
      }),
    );

    expect(received).not.toHaveBeenCalled();
  });
});
