import { describe, expect, it } from 'bun:test';

import { DEFAULT_CHAT_ROOM_CONFIG, withChatRoomDefaults } from './chat-defaults.js';

/* eslint-disable @typescript-eslint/no-explicit-any */

const derivedId = (source: string) => `0xderived-${source}`;

function fakeClient() {
  const calls: any[] = [];
  const posts: any[] = [];
  const footers: any[] = [];
  const client: any = {
    chat: {
      createRoom(request: any, options?: any) {
        calls.push({ request, options });
        const response = { status: 'New', roomId: derivedId(request.roomIdSource) };
        return { map: (next: (value: any) => any) => next(response) };
      },
      postMessage(request: any, options?: any) {
        posts.push({ request, options });
      },
      setRoomFooter(request: any, options?: any) {
        footers.push({ request, options });
      },
    },
  };
  return { client, calls, posts, footers };
}

describe('a config-less createRoom', () => {
  it('encodes with the host-store default config', () => {
    const { client, calls } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    wrapped.chat.createRoom({ roomIdSource: 'room', name: 'Room', icon: 'i.png' });
    expect(calls).toEqual([
      {
        request: {
          roomIdSource: 'room',
          name: 'Room',
          icon: 'i.png',
          config: DEFAULT_CHAT_ROOM_CONFIG,
        },
        options: undefined,
      },
    ]);
  });

  it('names the room from a v1 roomId when roomIdSource is missing', () => {
    const { client, calls } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    wrapped.chat.createRoom({ roomId: 'room', name: 'Room', icon: '' });
    expect(calls[0]?.request?.roomIdSource).toBe('room');
    expect(calls[0]?.request?.config).toEqual(DEFAULT_CHAT_ROOM_CONFIG);
  });

  it('lets a config the product sends win field by field', () => {
    const { client, calls } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    wrapped.chat.createRoom({
      roomIdSource: 'room',
      name: 'Room',
      icon: '',
      config: { productSource: true },
    });
    expect(calls[0]?.request?.config).toEqual({
      productSource: true,
      reactionDetails: false,
      reportSeen: false,
    });
  });

  it('passes call options through and returns the same client', () => {
    const { client, calls } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    expect(wrapped).toBe(client);
    const options = { signal: 's' };
    wrapped.chat.createRoom({ roomIdSource: 'room', name: 'Room', icon: '' }, options);
    expect(calls[0]?.options).toBe(options);
  });
});

describe('a room a v1 createRoom made', () => {
  it('gets the posts and footers the product sends under its own roomId', () => {
    const { client, posts, footers } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    wrapped.chat.createRoom({ roomId: 'room', name: 'Room', icon: '' });
    const options = { signal: 's' };
    wrapped.chat.postMessage({ roomId: 'room', payload: 'p', alt: 'a' }, options);
    wrapped.chat.setRoomFooter({ roomId: 'room', footer: 'Empty' });
    expect(posts).toEqual([
      { request: { roomId: derivedId('room'), payload: 'p', alt: 'a' }, options },
    ]);
    expect(footers).toEqual([
      { request: { roomId: derivedId('room'), footer: 'Empty' }, options: undefined },
    ]);
  });

  it('leaves the room ids of a v2 product and of unknown rooms as they are', () => {
    const { client, posts } = fakeClient();
    const wrapped = withChatRoomDefaults(client) as any;
    const created = wrapped.chat.createRoom({ roomIdSource: 'room', name: 'Room', icon: '' });
    wrapped.chat.postMessage({ roomId: created.roomId, payload: 'p' });
    wrapped.chat.postMessage({ roomId: 'room', payload: 'p' });
    wrapped.chat.postMessage({ roomId: 'elsewhere', payload: 'p' });
    expect(posts.map((post) => post.request.roomId)).toEqual([
      derivedId('room'),
      'room',
      'elsewhere',
    ]);
  });
});
