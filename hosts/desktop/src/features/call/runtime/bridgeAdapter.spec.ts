import { describe, expect, it, vi } from 'vitest';

import { createBridgeAdapter } from './bridgeAdapter';

// A valid MainToWindowMessage (deliverClosed has no sdp/candidates — simplest shape)
const validMsg = {
  kind: 'deliverClosed',
  offerId: 'test-offer-id',
};

// Something that is NOT a valid MainToWindowMessage
const invalidMsg = { kind: 'unknownKind', garbage: true };

describe('createBridgeAdapter', () => {
  it('routes a valid MainToWindowMessage to onMessage', async () => {
    const { port1, port2 } = new MessageChannel();
    const onMessage = vi.fn();
    createBridgeAdapter(port1, onMessage);

    port2.postMessage(validMsg);
    await new Promise<void>(resolve => setTimeout(resolve, 10));

    expect(onMessage).toHaveBeenCalledWith(expect.objectContaining({ kind: 'deliverClosed' }));
  });

  it('drops an invalid message (parseMainToWindow returns null)', async () => {
    const { port1, port2 } = new MessageChannel();
    const onMessage = vi.fn();
    createBridgeAdapter(port1, onMessage);

    port2.postMessage(invalidMsg);
    await new Promise<void>(resolve => setTimeout(resolve, 10));

    expect(onMessage).not.toHaveBeenCalled();
  });

  it('send posts a WindowToMainMessage on the port', async () => {
    const { port1, port2 } = new MessageChannel();
    const onMessage = vi.fn();
    const adapter = createBridgeAdapter(port1, onMessage);

    const received: unknown[] = [];
    port2.onmessage = e => received.push(e.data);
    port2.start();

    const msg = { kind: 'publishClosed' as const, offerId: 'oid' };
    adapter.send(msg);
    await new Promise<void>(resolve => setTimeout(resolve, 10));

    expect(received).toHaveLength(1);
    expect(received[0]).toMatchObject({ kind: 'publishClosed' });
  });
});
