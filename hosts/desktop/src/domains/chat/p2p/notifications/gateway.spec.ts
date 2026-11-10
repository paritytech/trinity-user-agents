import { x25519 } from '@noble/curves/ed25519.js';
import { createEncryption } from '@novasamatech/statement-store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { p2pService } from '../service';
import { type P2PRoom } from '../types';

import { pushNotificationGateway } from './gateway';
import { pushNotificationService } from './service';

// Plain caller-supplied values — the gateway takes backendUrl/iosBundleId as parameters,
// so nothing here has to stub the environment use case or a Remote Config fetch.
const IDENTITY_BACKEND = 'https://alpha-identity.example';
const IOS_BUNDLE = 'com.example.app';

const ALICE_ACCOUNT_ID = new Uint8Array(32).fill(0xaa);
const BOB_ACCOUNT_ID = new Uint8Array(32).fill(0xbb);

const ALICE_PRIV = new Uint8Array(32).fill(0x11);
const BOB_PRIV = new Uint8Array(32).fill(0x22);

const alice = { chatPrivateKey: ALICE_PRIV, chatPublicKey: x25519.getPublicKey(ALICE_PRIV) };
const bob = { chatPrivateKey: BOB_PRIV, chatPublicKey: x25519.getPublicKey(BOB_PRIV) };

describe('P2PRoom token fields', () => {
  it('accepts optional peerPushToken and peerPlatform', () => {
    const room: P2PRoom = {
      sessionId: 'peer-1',
      peerId: 'peer-1',
      peerUsername: 'alice',
      userId: 'me',
      createdAt: Date.now(),
      peerPushToken: 'abc123',
      peerPlatform: 'Android',
      lastUpdate: Date.now(),
    };
    expect(room.peerPushToken).toBe('abc123');
    expect(room.peerPlatform).toBe('Android');
  });
});

describe('computePushId', () => {
  it('returns a 32-byte Uint8Array', () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const pushId = pushNotificationService.computePushId(sharedSecret, ALICE_ACCOUNT_ID, BOB_ACCOUNT_ID);
    expect(pushId).toBeInstanceOf(Uint8Array);
    expect(pushId).toHaveLength(32);
  });

  it('produces deterministic output for same inputs', () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const pushId1 = pushNotificationService.computePushId(sharedSecret, ALICE_ACCOUNT_ID, BOB_ACCOUNT_ID);
    const pushId2 = pushNotificationService.computePushId(sharedSecret, ALICE_ACCOUNT_ID, BOB_ACCOUNT_ID);
    expect(pushId1).toEqual(pushId2);
  });

  it('produces different output when account order is swapped', () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const pushIdAB = pushNotificationService.computePushId(sharedSecret, ALICE_ACCOUNT_ID, BOB_ACCOUNT_ID);
    const pushIdBA = pushNotificationService.computePushId(sharedSecret, BOB_ACCOUNT_ID, ALICE_ACCOUNT_ID);
    expect(pushIdAB).not.toEqual(pushIdBA);
  });
});

describe('content predicates', () => {
  // Mirrors iOS `Chat.RemoteMessage.supportsNotification()`; keep both lists in step with it.
  it.each([
    'text',
    'richText',
    'send',
    'coinagePayment',
    'contactAdded',
    'leftChat',
    'reply',
    'reacted',
    'edit',
    'chatAccepted',
    'deviceChatAccepted',
    'dataChannelOffer',
  ])('treats %s as notifiable', tag => {
    expect(pushNotificationService.isNotifiableContent(tag)).toBe(true);
  });

  it.each([
    'token',
    'reactionRemoved',
    'dataChannelAnswer',
    'dataChannelIceCandidate',
    'dataChannelClosed',
    'deviceAdded',
    'deviceRemoved',
  ])('treats %s as non-notifiable', tag => {
    expect(pushNotificationService.isNotifiableContent(tag)).toBe(false);
  });

  it('fails closed on an unrecognised tag', () => {
    expect(pushNotificationService.isNotifiableContent('somethingNewOnTheWire')).toBe(false);
  });

  // iOS reports every VoIP push to CallKit as an incoming call, so only the offer may carry it.
  it('marks only a call offer as VoIP', () => {
    expect(pushNotificationService.isVoIPContent('dataChannelOffer')).toBe(true);
    expect(pushNotificationService.isVoIPContent('dataChannelAnswer')).toBe(false);
    expect(pushNotificationService.isVoIPContent('dataChannelIceCandidate')).toBe(false);
    expect(pushNotificationService.isVoIPContent('dataChannelClosed')).toBe(false);
  });
});

describe('getPlatformDeviceToken', () => {
  it('returns hex string as-is for iOS', () => {
    const apnsHex = 'aabb1122334455667788';
    expect(pushNotificationService.getPlatformDeviceToken(apnsHex, 'iOS')).toBe(apnsHex);
  });

  it('decodes hex-encoded UTF-8 back to raw string for Android', () => {
    // "dMf7token" → UTF-8 hex → should decode back
    const fcmToken = 'dMf7token';
    const hex = Array.from(new TextEncoder().encode(fcmToken))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');
    expect(pushNotificationService.getPlatformDeviceToken(hex, 'Android')).toBe(fcmToken);
  });

  it('returns hex string as-is for undefined platform', () => {
    const token = 'aabb1122';
    expect(pushNotificationService.getPlatformDeviceToken(token, undefined)).toBe(token);
  });
});

describe('sendPushNotification', () => {
  beforeEach(() => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ success: true, sent: 1, failed: 0 }),
      }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('sends POST request with correct body fields', async () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const encryption = createEncryption(sharedSecret);

    await pushNotificationGateway.sendPushNotification({
      deviceToken: 'abc123token',
      peerPlatform: 'iOS',
      sharedSecret,
      encryption,
      localAccountId: ALICE_ACCOUNT_ID,
      remoteAccountId: BOB_ACCOUNT_ID,
      messageId: 'msg-1',
      timestamp: 1000,
      content: { tag: 'text' as const, value: 'Hello!' },
      backendUrl: IDENTITY_BACKEND,
      iosBundleId: IOS_BUNDLE,
    });

    expect(fetch).toHaveBeenCalledOnce();
    const [url, options] = vi.mocked(fetch).mock.calls[0]!;
    expect(url).toBe(`${IDENTITY_BACKEND}/api/v1/notify`);
    expect(options?.method).toBe('POST');
    expect(options?.headers).toEqual({ 'Content-Type': 'application/json' });

    const body = JSON.parse(String(options?.body));
    expect(body.deviceToken).toBe('abc123token');
    expect(body.pushId).toHaveLength(64); // 32 bytes as hex
    expect(body.message).toBeTruthy(); // encrypted hex string
    expect(body.voip).toBe(false);
    expect(body.platform).toBe('ios');
    expect(body.bundlerId).toBe(IOS_BUNDLE);
  });

  it('includes platform and bundlerId for iOS peers', async () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const encryption = createEncryption(sharedSecret);

    await pushNotificationGateway.sendPushNotification({
      deviceToken: 'abc123token',
      peerPlatform: 'iOS',
      sharedSecret,
      encryption,
      localAccountId: ALICE_ACCOUNT_ID,
      remoteAccountId: BOB_ACCOUNT_ID,
      messageId: 'msg-ios',
      timestamp: 1000,
      content: { tag: 'text' as const, value: 'Hello!' },
      backendUrl: IDENTITY_BACKEND,
      iosBundleId: IOS_BUNDLE,
    });

    const body = JSON.parse(String(vi.mocked(fetch).mock.calls[0]![1]?.body));
    expect(body.platform).toBe('ios');
    expect(body.bundlerId).toBe(IOS_BUNDLE); // iOS → app bundle
  });

  it('converts Android device token from hex to UTF-8 string and sets platform', async () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const encryption = createEncryption(sharedSecret);
    const fcmToken = 'cFcmToken123';
    const hexToken = Array.from(new TextEncoder().encode(fcmToken))
      .map(b => b.toString(16).padStart(2, '0'))
      .join('');

    await pushNotificationGateway.sendPushNotification({
      deviceToken: hexToken,
      peerPlatform: 'Android',
      sharedSecret,
      encryption,
      localAccountId: ALICE_ACCOUNT_ID,
      remoteAccountId: BOB_ACCOUNT_ID,
      messageId: 'msg-android',
      timestamp: 1000,
      content: { tag: 'text' as const, value: 'Hello!' },
      backendUrl: IDENTITY_BACKEND,
      iosBundleId: IOS_BUNDLE,
    });

    const body = JSON.parse(String(vi.mocked(fetch).mock.calls[0]![1]?.body));
    expect(body.deviceToken).toBe(fcmToken);
    expect(body.platform).toBe('android');
    expect(body.bundlerId).toBeUndefined();
  });

  // iOS `NewMessageHandler` throws `unsupportedMessage` for every content its extension cannot
  // present, and renders an "Unsupported message" banner from the catch. Call signalling travels as
  // ordinary chat messages, so without this gate a single desktop-initiated call banners the peer
  // once per ICE batch, once for the answer and once for the hang-up.
  it.each(['dataChannelIceCandidate', 'dataChannelAnswer', 'dataChannelClosed', 'token', 'deviceAdded', 'deviceRemoved'])(
    'sends no request for non-notifiable %s content',
    async tag => {
      const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
      const encryption = createEncryption(sharedSecret);

      await pushNotificationGateway.sendPushNotification({
        deviceToken: 'abc123token',
        peerPlatform: 'iOS',
        sharedSecret,
        encryption,
        localAccountId: ALICE_ACCOUNT_ID,
        remoteAccountId: BOB_ACCOUNT_ID,
        messageId: `msg-${tag}`,
        timestamp: 1000,
        // eslint-disable-next-line @typescript-eslint/no-explicit-any, @typescript-eslint/consistent-type-assertions -- the point of the case is a tag the notifiable set rejects
        content: { tag, value: {} } as any,
        backendUrl: IDENTITY_BACKEND,
        iosBundleId: IOS_BUNDLE,
      });

      expect(fetch).not.toHaveBeenCalled();
    },
  );

  it('sets voip to true for callOffer content', async () => {
    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const encryption = createEncryption(sharedSecret);

    await pushNotificationGateway.sendPushNotification({
      deviceToken: 'abc123token',
      peerPlatform: 'iOS',
      sharedSecret,
      encryption,
      localAccountId: ALICE_ACCOUNT_ID,
      remoteAccountId: BOB_ACCOUNT_ID,
      messageId: 'msg-2',
      timestamp: 1000,
      content: { tag: 'dataChannelOffer' as const, value: { sdp: new Uint8Array(), purpose: 'AUDIO_CALL' as const } },
      backendUrl: IDENTITY_BACKEND,
      iosBundleId: IOS_BUNDLE,
    });

    const body = JSON.parse(String(vi.mocked(fetch).mock.calls[0]![1]?.body));
    expect(body.voip).toBe(true);
  });

  it('does not throw when fetch fails', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Network error')));

    const sharedSecret = p2pService.computeSharedSecret(alice.chatPrivateKey, bob.chatPublicKey);
    const encryption = createEncryption(sharedSecret);

    await pushNotificationGateway.sendPushNotification({
      deviceToken: 'abc123token',
      peerPlatform: undefined,
      sharedSecret,
      encryption,
      localAccountId: ALICE_ACCOUNT_ID,
      remoteAccountId: BOB_ACCOUNT_ID,
      messageId: 'msg-3',
      timestamp: 1000,
      content: { tag: 'text' as const, value: 'Hello!' },
      backendUrl: IDENTITY_BACKEND,
      iosBundleId: IOS_BUNDLE,
    });
  });
});
