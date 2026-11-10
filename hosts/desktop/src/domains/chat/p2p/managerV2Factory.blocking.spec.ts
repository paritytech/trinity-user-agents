import 'fake-indexeddb/auto';

import { beforeEach, describe, expect, it, vi } from 'vitest';

import { environmentUseCase } from '@/domains/application';
import { contactRepository } from '@/domains/contact';

import { pushNotificationGateway } from './notifications/gateway';
import { peerGateway } from './peer/gateway';
import { p2pChatDatabase } from './repository';
import { chatRequestGateway } from './requests/gateway';

const OWN_USER_ID = '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty';
const PEER_ID = '5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY';

// The callback a blocked contact's messages arrive on, captured so tests can drive it.
let capturedOnMessage: ((msg: { messageId: string; timestamp: number; content: unknown }) => void) | null = null;
const sessionDispose = vi.fn();
const identityChannelDispose = vi.fn();
const createSessionSpy = vi.fn();
const createIdentityChannelSpy = vi.fn();

vi.mock('./chatSessionV2', () => ({
  createChatPeerSessionV2: (params: { onMessage: (msg: { messageId: string; timestamp: number; content: unknown }) => void }) => {
    createSessionSpy();
    capturedOnMessage = params.onMessage;
    return { send: vi.fn().mockResolvedValue({ messageId: 'm', timestamp: 1 }), dispose: sessionDispose };
  },
  isMessageTooLargeError: () => false,
}));

vi.mock('./identityChannel', () => ({
  createIdentityChannel: () => {
    createIdentityChannelSpy();

    return { post: vi.fn().mockResolvedValue(undefined), dispose: identityChannelDispose };
  },
}));

// `trackedSubscribeStatements` is a bare function export, which nothing can spy on in
// place — the module has to be replaced until the manager takes its transport as a parameter.
vi.mock('./subscription-registry', () => ({
  trackedSubscribeStatements: vi.fn().mockReturnValue(() => {}),
}));

// Every wire gateway is a plain object, spied in place so no statement, request or push
// leaves the process; the active environment is spied the same way. The device-sync
// repository runs for real on the fake IndexedDB imported above.
vi.spyOn(chatRequestGateway, 'subscribeToIncomingRequestsV2').mockReturnValue(() => {});
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(chatRequestGateway, 'sendChatRequestV2').mockResolvedValue(undefined as never);
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(peerGateway, 'createPeerResolver').mockReturnValue({
  searchUsers: vi.fn().mockResolvedValue([]),
  getUsername: vi.fn().mockResolvedValue(undefined),
  getPeerContact: vi.fn().mockResolvedValue(null),
  getPeerChatKey: vi.fn().mockResolvedValue(new Uint8Array(32).fill(0xcc)),
} as never);
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(pushNotificationGateway, 'sendPushNotification').mockResolvedValue(undefined as never);
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(environmentUseCase, 'getActive').mockResolvedValue({ bulletinHopEndpoints: [''] } as never);
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(environmentUseCase, 'getActiveId').mockReturnValue('test-env' as never);
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- the manager reads only these fields of the double
vi.spyOn(environmentUseCase, 'getById').mockResolvedValue({ backendUrl: 'https://example.invalid' } as never);

const { createP2PChatManagerV2 } = await import('./managerV2Factory');

const key = (fill: number) => new Uint8Array(32).fill(fill);

const buildManager = () =>
  createP2PChatManagerV2({
    // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/consistent-type-assertions -- test double for the SDK adapter; only the mocked session touches it
    statementStore: {} as any,
    userId: OWN_USER_ID,
    device: {
      statementAccountPublicKey: key(0x11),
      statementAccountSeed: new Uint8Array(64).fill(0x22),
      encryptionPrivateKey: key(0x33),
      encryptionPublicKey: key(0x44),
    },
    userIdentity: {
      identitySr25519PublicKey: key(0x55),
      identityChatPrivateKey: key(0x66),
      identityChatPublicKey: key(0x67),
      rootSr25519PublicKey: null,
      peerDeviceEncPubKey: key(0x68),
      peerDeviceStatementAccountId: key(0x69),
      ssoEncPubKey: null,
    },
  });

const seedRoom = async (isBlocked: boolean) => {
  await p2pChatDatabase.rooms.put({
    sessionId: PEER_ID,
    peerId: PEER_ID,
    peerUsername: 'blocked-peer',
    userId: OWN_USER_ID,
    createdAt: 1,
    isBlocked,
    lastUpdate: 1,
  });
  await contactRepository.upsert({
    accountId: PEER_ID,
    identityChatPublicKey: `0x${'88'.repeat(32)}`,
    devices: [{ statementAccountId: `0x${'99'.repeat(32)}`, encryptionPublicKey: `0x${'aa'.repeat(32)}` }],
  });
};

describe('managerV2Factory blocking enforcement', () => {
  beforeEach(async () => {
    await p2pChatDatabase.rooms.clear();
    await p2pChatDatabase.messages.clear();
    await p2pChatDatabase.requests.clear();
    await contactRepository.clearAll();
    capturedOnMessage = null;
    sessionDispose.mockClear();
    identityChannelDispose.mockClear();
    createSessionSpy.mockClear();
    createIdentityChannelSpy.mockClear();
  });

  it('drops an incoming message from a blocked peer instead of writing it to the chat', async () => {
    await seedRoom(false);
    const manager = await buildManager();
    await manager.initialize();

    expect(capturedOnMessage).toBeTypeOf('function');

    await manager.setBlocked(PEER_ID, true);

    capturedOnMessage?.({ messageId: 'msg-while-blocked', timestamp: 42, content: { tag: 'text', value: 'hello' } });
    await vi.waitFor(async () => {
      expect(await p2pChatDatabase.messages.count()).toBe(0);
    });

    expect(await p2pChatDatabase.messages.get('msg-while-blocked')).toBeUndefined();
  });

  it('still writes an incoming message when the peer is not blocked', async () => {
    await seedRoom(false);
    const manager = await buildManager();
    await manager.initialize();

    capturedOnMessage?.({ messageId: 'msg-allowed', timestamp: 42, content: { tag: 'text', value: 'hello' } });

    await vi.waitFor(async () => {
      expect(await p2pChatDatabase.messages.get('msg-allowed')).toBeDefined();
    });
  });

  it('tears down the session and identity channel on block, and keeps the room and its messages', async () => {
    await seedRoom(false);
    const manager = await buildManager();
    await manager.initialize();

    await manager.setBlocked(PEER_ID, true);

    expect(sessionDispose).toHaveBeenCalled();
    expect(identityChannelDispose).toHaveBeenCalled();
    // Unlike leaving a chat, the history survives — the unblock banner renders over it.
    expect(await p2pChatDatabase.rooms.get(PEER_ID)).toBeDefined();
    expect(await contactRepository.get(PEER_ID)).toBeDefined();
  });

  it('does not start a session for a room that is already blocked at initialize', async () => {
    await seedRoom(true);
    const manager = await buildManager();
    await manager.initialize();

    expect(createSessionSpy).not.toHaveBeenCalled();
  });

  it('restarts the session on unblock', async () => {
    await seedRoom(true);
    const manager = await buildManager();
    await manager.initialize();
    expect(createSessionSpy).not.toHaveBeenCalled();

    await manager.setBlocked(PEER_ID, false);

    expect(createSessionSpy).toHaveBeenCalled();
  });

  it('does not leave the peer blocked after the chat is left and re-established', async () => {
    // `removeSession` deletes the room the block flag lives on; a mirror that outlived
    // it would make the re-paired chat silently dead.
    await seedRoom(false);
    const manager = await buildManager();
    await manager.initialize();

    await manager.setBlocked(PEER_ID, true);
    await manager.removeSession(PEER_ID);

    await seedRoom(false);
    createSessionSpy.mockClear();
    await manager.startSession(PEER_ID, 'blocked-peer');

    expect(createSessionSpy).toHaveBeenCalled();
  });

  it('restores both transports when a sibling deletes the room of a blocked peer', async () => {
    // A sibling's `ChatsRemoved` deletes the room through the applier, behind the
    // manager's back — so nothing clears the in-memory mirror. The identity channel
    // must still come up on re-pair; it starts before `startSession`, so a mirror
    // consulted here would suppress it silently and cost us the peer's roster fan-out.
    await seedRoom(false);
    const manager = await buildManager();
    await manager.initialize();

    await manager.setBlocked(PEER_ID, true);
    // The applier's `ChatsRemoved` path — a direct row delete, not `removeSession`,
    // so nothing clears the in-memory mirror.
    await p2pChatDatabase.rooms.delete(PEER_ID);

    // The peer re-requests and we accept. `acceptRequest` starts the identity channel
    // before `startSession` — the only thing that read-throughs the mirror — and without
    // `senderDevicePubKey` that `startSession` never runs. So a mirror consulted here
    // suppresses the channel for the whole call, and the `deviceChatAccepted` post that
    // rides it is silently dropped: the peer never learns we accepted.
    await p2pChatDatabase.requests.put({
      requestId: 're-request',
      peerId: PEER_ID,
      peerUsername: 'blocked-peer',
      direction: 'incoming',
      status: 'pending',
      timestamp: 2,
      channelTopic: '0xbb',
      userId: OWN_USER_ID,
      lastUpdate: 2,
    });
    createIdentityChannelSpy.mockClear();

    await manager.acceptRequest('re-request');

    expect(createIdentityChannelSpy).toHaveBeenCalled();
  });

  it('refuses to send to a blocked peer', async () => {
    await seedRoom(true);
    const manager = await buildManager();
    await manager.initialize();

    await expect(manager.sendMessage(PEER_ID, { type: 'text', text: 'hi' })).rejects.toThrow(/blocked peer/);
  });
});
