import 'fake-indexeddb/auto';
import { Subject, defer, firstValueFrom, of } from 'rxjs';
import { parse } from 'valibot';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { accountId } from '@/domains/network';
import { type ProductChatRoom } from '../product/types';
import { type ChatMessage } from '../session/types';

const persisted: ChatMessage[] = [];
const roomsSubject = new Subject<ProductChatRoom[]>();
const roomsReadParams: { accountId: string }[] = [];

// The four writes are stubbed so the test can observe them.
vi.mock(import('../product/resource'), async importOriginal => ({
  ...(await importOriginal()),
  createMessageInProductRoom: (message: ChatMessage) => {
    persisted.push(message);

    return of({ messageId: message.messageId });
  },
  deleteMessagesInProductRoom: () => of(null),
  deleteProductRoom: () => of(null),
  markProductMessagesAsRead: () => of(null),
}));

const { productRoomUseCase } = await import('./productRoom');
const { messagesResource, roomsResource } = await import('../product/resource');
const { chatDatabase } = await import('../product/repository');
const { productChatService } = await import('../product/service');

// Keyed per room: every session object for the same room observes one stream.
function seedResources() {
  messagesResource.instead(({ sessionId }) => defer(() => of(persisted.filter(m => m.sessionId === sessionId))));
  roomsResource.instead(params => {
    roomsReadParams.push({ accountId: params.accountId });

    return roomsSubject.asObservable();
  });
}

const userId = parse(accountId, '0xuser');

const peer = { type: 'user' as const, accountId: userId, name: 'Alice' };

const room: ProductChatRoom = {
  sessionId: 'product:coinflip:room-1:0xuser',
  roomId: 'room-1',
  productId: 'coinflipgame03.dot',
  userId,
  createdAt: 1000,
};

describe('watchProductRooms', () => {
  beforeEach(() => {
    seedResources();
    roomsReadParams.length = 0;
  });

  it('reads the resource keyed by the requesting user', async () => {
    const emission = firstValueFrom(productRoomUseCase.watchProductRooms({ productId: 'alpha.dot', userId }));
    roomsSubject.next([]);
    await emission;

    expect(roomsReadParams).toEqual([{ accountId: userId }]);
  });

  it('emits only the rooms of the requested product', async () => {
    const emission = firstValueFrom(productRoomUseCase.watchProductRooms({ productId: 'alpha.dot', userId }));
    roomsSubject.next([
      { sessionId: '0xa', roomId: 'room-a', productId: 'alpha.dot', userId, createdAt: 1 },
      { sessionId: '0xb', roomId: 'room-b', productId: 'beta.dot', userId, createdAt: 2 },
    ]);

    await expect(emission).resolves.toEqual([
      { sessionId: '0xa', roomId: 'room-a', productId: 'alpha.dot', userId, createdAt: 1 },
    ]);
  });

  it('tracks live inserts, so a room added after subscribe still reaches the worker', () => {
    const seen: ProductChatRoom[][] = [];
    const subscription = productRoomUseCase
      .watchProductRooms({ productId: 'alpha.dot', userId })
      .subscribe(rooms => seen.push(rooms));

    roomsSubject.next([]);
    roomsSubject.next([{ sessionId: '0xa', roomId: 'room-a', productId: 'alpha.dot', userId, createdAt: 1 }]);
    subscription.unsubscribe();

    expect(seen).toHaveLength(2);
    expect(seen[1]).toHaveLength(1);
  });
});

describe('watchOutgoingMessages', () => {
  let messageSubjects: Map<string, Subject<ChatMessage[]>>;

  function subjectFor(sessionId: string) {
    let subject = messageSubjects.get(sessionId);
    if (!subject) {
      subject = new Subject<ChatMessage[]>();
      messageSubjects.set(sessionId, subject);
    }

    return subject;
  }

  const room2: ProductChatRoom = { ...room, roomId: 'room-2', sessionId: 'product:coinflip:room-2:0xuser' };
  const outgoing: ChatMessage = {
    messageId: 'm1',
    sessionId: room.sessionId,
    peer,
    content: { type: 'text', text: 'Hi' },
    timestamp: 1,
    status: { direction: 'outgoing', state: 'sent' },
  };

  beforeEach(() => {
    seedResources();
    messageSubjects = new Map();
    messagesResource.instead(({ sessionId }) => subjectFor(sessionId));
  });

  it('delivers a message once after the room list re-emits', () => {
    const collected: unknown[] = [];
    const subscription = productRoomUseCase
      .watchOutgoingMessages({ productId: room.productId, userId })
      .subscribe(message => collected.push(message));

    roomsSubject.next([room]);
    subjectFor(room.sessionId).next([]);
    roomsSubject.next([{ ...room, userInput: 'none' }]);
    roomsSubject.next([{ ...room, userInput: 'text' }]);
    subjectFor(room.sessionId).next([outgoing]);
    subscription.unsubscribe();

    expect(collected).toHaveLength(1);
  });

  it('delivers a message once after another room is added', () => {
    const collected: unknown[] = [];
    const subscription = productRoomUseCase
      .watchOutgoingMessages({ productId: room.productId, userId })
      .subscribe(message => collected.push(message));

    roomsSubject.next([room]);
    subjectFor(room.sessionId).next([]);
    roomsSubject.next([room, room2]);
    subjectFor(room.sessionId).next([outgoing]);
    subscription.unsubscribe();

    expect(collected).toHaveLength(1);
  });

  it('still picks up a room added after subscribe', () => {
    const collected: unknown[] = [];
    const subscription = productRoomUseCase
      .watchOutgoingMessages({ productId: room.productId, userId })
      .subscribe(message => collected.push(message));

    roomsSubject.next([]);
    roomsSubject.next([room]);
    subjectFor(room.sessionId).next([]);
    subjectFor(room.sessionId).next([outgoing]);
    subscription.unsubscribe();

    expect(collected).toHaveLength(1);
  });
});

describe('createProductChatSession', () => {
  beforeEach(() => {
    seedResources();
    persisted.length = 0;
  });

  it('surfaces a send on a session object built independently of the sender', async () => {
    // Chat UI and ProductWorker build separate session objects for the same room;
    // a send through one must be observable through the other.
    const uiSession = productRoomUseCase.createProductChatSession(peer, room);
    const workerSession = productRoomUseCase.createProductChatSession(peer, room);

    await uiSession.sendMessage({ type: 'text', text: 'Hello!' });

    const messages = await firstValueFrom(workerSession.messages);
    expect(messages.map(m => m.content)).toContainEqual({ type: 'text', text: 'Hello!' });
  });

  it('persists a send as outgoing so it is distinguishable from a worker reply', async () => {
    const session = productRoomUseCase.createProductChatSession(peer, room);

    const { messageId } = await session.sendMessage({ type: 'text', text: 'Ping' });

    expect(persisted.find(m => m.messageId === messageId)?.status.direction).toBe('outgoing');
  });

  it('scopes messages to their own room', async () => {
    const otherRoom: ProductChatRoom = { ...room, sessionId: 'product:coinflip:room-2:0xuser', roomId: 'room-2' };

    const session = productRoomUseCase.createProductChatSession(peer, room);
    const otherSession = productRoomUseCase.createProductChatSession(peer, otherRoom);

    await session.sendMessage({ type: 'text', text: 'Only room 1' });

    const messages = await firstValueFrom(otherSession.messages);
    expect(messages.map(m => m.content)).not.toContainEqual({ type: 'text', text: 'Only room 1' });
  });
});

describe('createProductChatSession userInput', () => {
  const otherPeer = { ...peer, accountId: parse(accountId, '0xother') };

  beforeEach(() => {
    seedResources();
    roomsReadParams.length = 0;
  });

  it('defaults to text for a room without a setting', async () => {
    const session = productRoomUseCase.createProductChatSession(otherPeer, room);
    const first = firstValueFrom(session.userInput!);
    roomsSubject.next([room]);

    await expect(first).resolves.toBe('text');
  });

  it('follows the stored setting live and skips repeats', () => {
    const session = productRoomUseCase.createProductChatSession(otherPeer, room);
    const seen: string[] = [];
    const subscription = session.userInput!.subscribe(value => seen.push(value));

    roomsSubject.next([room]);
    roomsSubject.next([{ ...room, userInput: 'none' }]);
    roomsSubject.next([{ ...room, userInput: 'none' }]);
    subscription.unsubscribe();

    expect(seen).toEqual(['text', 'none']);
  });

  it("reads the rooms of the room's own user", () => {
    const session = productRoomUseCase.createProductChatSession(otherPeer, room);
    const subscription = session.userInput!.subscribe();
    subscription.unsubscribe();

    expect(roomsReadParams).toEqual([{ accountId: room.userId }]);
  });
});

describe('setProductRoomUserInput', () => {
  const stored = {
    ...room,
    sessionId: productChatService.getSessionId(room.productId, room.roomId, room.userId),
  };
  const params = { productId: room.productId, roomId: room.roomId, userId: room.userId };

  afterEach(async () => {
    await chatDatabase.rooms.clear();
  });

  it('stores the setting on an existing room', async () => {
    await chatDatabase.rooms.add(stored);

    await expect(productRoomUseCase.setProductRoomUserInput({ ...params, userInput: 'none' })).resolves.toBe('Applied');
    expect((await chatDatabase.rooms.get(stored.sessionId))?.userInput).toBe('none');
  });

  it('resolves Applied when the room already has that setting', async () => {
    await chatDatabase.rooms.add({ ...stored, userInput: 'none' });

    await expect(productRoomUseCase.setProductRoomUserInput({ ...params, userInput: 'none' })).resolves.toBe('Applied');
  });

  it('switches back to text', async () => {
    await chatDatabase.rooms.add({ ...stored, userInput: 'none' });

    await expect(productRoomUseCase.setProductRoomUserInput({ ...params, userInput: 'text' })).resolves.toBe('Applied');
    expect((await chatDatabase.rooms.get(stored.sessionId))?.userInput).toBe('text');
  });

  it('reports UnknownRoom and writes nothing when the room does not exist', async () => {
    await expect(productRoomUseCase.setProductRoomUserInput({ ...params, userInput: 'none' })).resolves.toBe('UnknownRoom');
    await expect(chatDatabase.rooms.count()).resolves.toBe(0);
  });

  it('reports UnknownRoom for the same room id under another user', async () => {
    const otherSessionId = productChatService.getSessionId(room.productId, room.roomId, parse(accountId, '0xother'));
    await chatDatabase.rooms.add({ ...stored, sessionId: otherSessionId });

    await expect(productRoomUseCase.setProductRoomUserInput({ ...params, userInput: 'none' })).resolves.toBe('UnknownRoom');
    expect((await chatDatabase.rooms.get(otherSessionId))?.userInput).toBeUndefined();
  });
});
