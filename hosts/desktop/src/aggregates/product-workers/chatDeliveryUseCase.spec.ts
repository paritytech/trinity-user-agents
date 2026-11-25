import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type OutgoingProductMessage, productRoomUseCase } from '@/domains/chat';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { chatDeliveryUseCase } from './chatDeliveryUseCase';

// The outgoing stream comes from Dexie; spied in place so each case pushes the messages
// it needs instead of seeding rows.
const watchOutgoingMessagesMock = vi.spyOn(productRoomUseCase, 'watchOutgoingMessages');

const USER: `0x${string}` = `0x${'11'.repeat(32)}`;
const PRODUCT = 'demo.dot';

function makeProvider() {
  const publishChatAction = vi.fn(() => Promise.resolve());

  // Only `publishChatAction` is reached; the rest of the provider is never called.
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- see above
  return { provider: { publishChatAction } as unknown as Parameters<typeof chatDeliveryUseCase.start>[1], publishChatAction };
}

function connect() {
  truapiRuntimeUseCase.publishAuthState({ tag: 'Connected', value: { publicKey: USER, identityAccountId: USER } });
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  truapiRuntimeUseCase.dispose();
});

describe('chatDeliveryUseCase.start', () => {
  it('publishes a user message into the product action stream', () => {
    const messages = new Subject<OutgoingProductMessage>();
    watchOutgoingMessagesMock.mockReturnValue(messages);
    connect();

    const { provider, publishChatAction } = makeProvider();
    const stop = chatDeliveryUseCase.start(PRODUCT, provider);

    messages.next({ roomId: 'r1', peerId: USER, content: { type: 'text', text: 'hi' } });

    expect(publishChatAction).toHaveBeenCalledWith({
      roomId: 'r1',
      peer: USER,
      payload: { tag: 'MessagePosted', value: { tag: 'Text', value: { text: 'hi' } } },
    });

    stop();
  });

  // The domain records kinds the wire has no word for. Sending a made-up shape would
  // tell the product something the protocol cannot express.
  it('drops a content kind the wire cannot carry', () => {
    const messages = new Subject<OutgoingProductMessage>();
    watchOutgoingMessagesMock.mockReturnValue(messages);
    connect();

    const { provider, publishChatAction } = makeProvider();
    const stop = chatDeliveryUseCase.start(PRODUCT, provider);

    messages.next({ roomId: 'r1', peerId: USER, content: { type: 'leftChat' } });

    expect(publishChatAction).not.toHaveBeenCalled();

    stop();
  });

  it('delivers nothing while no session is connected', () => {
    const messages = new Subject<OutgoingProductMessage>();
    watchOutgoingMessagesMock.mockReturnValue(messages);
    truapiRuntimeUseCase.publishAuthState({ tag: 'Disconnected' });

    const { provider, publishChatAction } = makeProvider();
    const stop = chatDeliveryUseCase.start(PRODUCT, provider);

    messages.next({ roomId: 'r1', peerId: USER, content: { type: 'text', text: 'hi' } });

    expect(publishChatAction).not.toHaveBeenCalled();
    expect(watchOutgoingMessagesMock).not.toHaveBeenCalled();

    stop();
  });

  // Rooms are stored per user; a loop left on the previous identity would forward one
  // user's messages into a session opened for another.
  it('re-keys the delivery loop when the identity changes', () => {
    watchOutgoingMessagesMock.mockReturnValue(new Subject<OutgoingProductMessage>());
    connect();

    const { provider } = makeProvider();
    const stop = chatDeliveryUseCase.start(PRODUCT, provider);

    expect(watchOutgoingMessagesMock).toHaveBeenCalledWith({ productId: PRODUCT, userId: USER });

    const OTHER: `0x${string}` = `0x${'22'.repeat(32)}`;
    truapiRuntimeUseCase.publishAuthState({ tag: 'Connected', value: { publicKey: OTHER, identityAccountId: OTHER } });

    expect(watchOutgoingMessagesMock).toHaveBeenLastCalledWith({ productId: PRODUCT, userId: OTHER });

    stop();
  });

  it('stops delivering once the worker is torn down', () => {
    const messages = new Subject<OutgoingProductMessage>();
    watchOutgoingMessagesMock.mockReturnValue(messages);
    connect();

    const { provider, publishChatAction } = makeProvider();
    chatDeliveryUseCase.start(PRODUCT, provider)();

    messages.next({ roomId: 'r1', peerId: USER, content: { type: 'text', text: 'hi' } });

    expect(publishChatAction).not.toHaveBeenCalled();
  });
});
