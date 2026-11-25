import 'fake-indexeddb/auto';

import { x25519 } from '@noble/curves/ed25519.js';
import { Subject } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { startDeviceSyncOrchestrator } from './orchestrator';
import { deviceSyncDatabase, deviceSyncRepository } from './repository';

// Capture per-peer phase transitions so a test can assert the error escalation.
// The orchestrator reports every peer's phase to ./resource; mocking the module
// also keeps the real status interval/stream from spinning up during the test.
const { phaseSpy } = vi.hoisted(() => ({ phaseSpy: vi.fn((_peerId: string, _phase: string) => {}) }));
vi.mock('./resource', () => ({
  reportPeerSyncPhase: (peerId: string, phase: string) => phaseSpy(peerId, phase),
}));

// Count live RTCPeerConnection constructions so a test can assert respawns.
let liveRtcCount = 0;
// The most recently created data channel, so a test can drive its open/close.
let lastDataChannel: { readyState: string; fire: (ev: string) => void } | null = null;

// Mock RTCPeerConnection so the orchestrator can construct PeerConnections
class FakeRtc extends EventTarget {
  localDescription = null;
  remoteDescription = null;
  signalingState: RTCSignalingState = 'stable';
  iceConnectionState: RTCIceConnectionState = 'new';
  // The data channel stays 'connecting' and never fires 'open' — modelling a
  // stalled handshake (initiator sent an Offer, no usable Answer ever arrives).
  createDataChannel = vi.fn(() => {
    const listeners: Record<string, (() => void)[]> = {};
    const channel = {
      label: 'sync',
      readyState: 'connecting',
      addEventListener: (ev: string, cb: () => void) => {
        (listeners[ev] ??= []).push(cb);
      },
      removeEventListener: (ev: string, cb: () => void) => {
        listeners[ev] = (listeners[ev] ?? []).filter(c => c !== cb);
      },
      fire: (ev: string) => {
        for (const cb of listeners[ev] ?? []) cb();
      },
      send: vi.fn(),
      close: vi.fn(),
    };
    lastDataChannel = channel;

    return channel;
  });
  createOffer = vi.fn(() => Promise.resolve({ type: 'offer' as const, sdp: 'fake' }));
  createAnswer = vi.fn(() => Promise.resolve({ type: 'answer' as const, sdp: 'fake' }));
  setLocalDescription = vi.fn(async () => {});
  setRemoteDescription = vi.fn(async () => {});
  addIceCandidate = vi.fn(async () => {});
  close = vi.fn();

  constructor() {
    super();
    liveRtcCount++;
  }
}

// A real X25519 keypair so createDeviceSessionChannel's getSharedSecret succeeds
// for a non-self peer (the only way an actual signaler + PC gets spawned).
const PEER_ENC_PRIV = new Uint8Array(32).fill(0x20);
const PEER_ENC_PUB = x25519.getPublicKey(PEER_ENC_PRIV);

// Own-device keys must also be a real X25519 pair now: the orchestrator
// validates every encryptionPublicKey it seeds/spawns and skips non-points.
const OWN_ENC_PRIV = new Uint8Array(32).fill(0x10);
const OWN_ENC_PUB = x25519.getPublicKey(OWN_ENC_PRIV);

beforeEach(async () => {
  liveRtcCount = 0;
  lastDataChannel = null;
  phaseSpy.mockClear();
  // @ts-expect-error global injection for test
  globalThis.RTCPeerConnection = FakeRtc;
  await deviceSyncDatabase.knownUserDevices.clear();
  await deviceSyncDatabase.syncConnectionMeta.clear();
});

describe('startDeviceSyncOrchestrator', () => {
  // The self entry is seeded too; filtering it out happens at the active-peers query.
  it('seeds knownUserDevices from the initial peer list', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      // Only the self entry — keeps getSharedSecret out of the way; that path is
      // covered by signaler / device-session unit tests.
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x01),
            encryptionPublicKey: OWN_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    const stored = await deviceSyncRepository.list();
    expect(stored.length).toBe(1);
    expect(stored[0]?.status).toBe('active');

    // Self is excluded from listActivePeers, so no peer signalers are spawned.
    const active = await deviceSyncRepository.listActivePeers(stored[0]!.statementAccountId);
    expect(active).toHaveLength(0);

    handle.stop();
  });

  it('skips seeding a peer whose encryptionPublicKey is not a 32-byte X25519 key', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      // Wrong-width encryption key — must not be persisted or spawned.
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: new Uint8Array(65).fill(0x3b),
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    expect(await deviceSyncRepository.list()).toHaveLength(0);
    expect(liveRtcCount).toBe(0);

    handle.stop();
  });

  it('purges a persisted peer row whose encryptionPublicKey is invalid and still spawns the valid peer', async () => {
    // Poisoned row: a wrong-width value stored where an X25519 key belongs.
    await deviceSyncRepository.upsert({
      statementAccountId: '0x' + '3b'.repeat(32),
      encryptionPublicKey: '0x' + '3b'.repeat(65),
      status: 'active',
      lastUpdate: Date.now(),
      outgoingUpdateTime: 0,
    });

    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: PEER_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    // The poisoned row is gone (so chat fanout can't ship it either)…
    const stored = await deviceSyncRepository.list();
    expect(stored.map(d => d.statementAccountId)).toEqual(['0x' + '02'.repeat(32)]);
    // …and the valid peer still got a connection (one bad row must not take
    // down sync for everyone — the old behavior threw out of the whole start).
    expect(liveRtcCount).toBe(1);

    handle.stop();
  });

  it('returns a handle whose stop() is callable without active peers', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () => Promise.resolve([]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    expect(typeof handle.stop).toBe('function');
    handle.stop();
  });

  it('sends a restart-recovery signal on first spawn when a peer has a persisted lastOfferId', async () => {
    // own (0x05) > peer (0x02) → Desktop is the ACCEPTOR, so its signaler sends
    // NOTHING on spawn — the only statement posted is the restart-recovery
    // Reconnected, which lets us assert the recovery fired without decrypting.
    const peerHex = '0x' + '02'.repeat(32);
    await deviceSyncRepository.upsert({
      statementAccountId: peerHex,
      encryptionPublicKey: '0x' + Buffer.from(PEER_ENC_PUB).toString('hex'),
      status: 'active',
      lastUpdate: 1,
      outgoingUpdateTime: 0,
      lastOfferId: 'persisted-attempt',
    });

    const postStatement = vi.fn(async () => {});
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x05),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([{ statementAccountId: new Uint8Array(32).fill(0x02), encryptionPublicKey: PEER_ENC_PUB }]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement,
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    await new Promise(r => setTimeout(r, 50));
    // The acceptor sends nothing on its own; this post is the recovery signal.
    expect(postStatement).toHaveBeenCalled();
    handle.stop();
  });

  it('does not send a restart-recovery signal when the peer has no persisted lastOfferId', async () => {
    const peerHex = '0x' + '02'.repeat(32);
    await deviceSyncRepository.upsert({
      statementAccountId: peerHex,
      encryptionPublicKey: '0x' + Buffer.from(PEER_ENC_PUB).toString('hex'),
      status: 'active',
      lastUpdate: 1,
      outgoingUpdateTime: 0,
    });

    const postStatement = vi.fn(async () => {});
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x05),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([{ statementAccountId: new Uint8Array(32).fill(0x02), encryptionPublicKey: PEER_ENC_PUB }]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement,
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
    });

    await new Promise(r => setTimeout(r, 50));
    // Acceptor with nothing to recover stays silent until it receives an Offer.
    expect(postStatement).not.toHaveBeenCalled();
    handle.stop();
  });

  // The timeout matches Android's CONNECT_TIMEOUT so both ends give up together.
  it('respawns the peer connection when the data channel does not open within the handshake timeout', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      // own (0x01) < peer (0x02) → Desktop is the initiator for this pair.
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: PEER_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
      handshakeTimeoutMs: 80,
      respawnBackoffMs: 10,
    });

    // Exactly one PC right after the initial spawn.
    expect(liveRtcCount).toBe(1);

    // The DC never opens; after ~3 handshake-timeout cycles the orchestrator
    // should have torn down and rebuilt the PC several times.
    await new Promise(r => setTimeout(r, 300));
    const countWhileRunning = liveRtcCount;
    expect(countWhileRunning).toBeGreaterThan(1);

    // stop() must clear the pending handshake timer so respawns cease.
    handle.stop();
    await new Promise(r => setTimeout(r, 200));
    expect(liveRtcCount).toBe(countWhileRunning);
  });

  it('does not spawn or submit when the start signal is already aborted (superseded mid-flight)', async () => {
    const controller = new AbortController();
    controller.abort();
    const postStatement = vi.fn(async () => {});

    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: PEER_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement,
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
      signal: controller.signal,
    });

    // A superseded start seeds the repository but never spawns a connection or
    // submits signaling — so it can't race a newer orchestrator on the account.
    expect(liveRtcCount).toBe(0);
    expect(postStatement).not.toHaveBeenCalled();

    handle.stop();
  });

  it('tears down a running orchestrator when the start signal aborts after spawn', async () => {
    const controller = new AbortController();

    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: PEER_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
      handshakeTimeoutMs: 80,
      respawnBackoffMs: 10,
      signal: controller.signal,
    });

    expect(liveRtcCount).toBe(1);

    // Aborting tears the spawned connection down and stops respawn timers — no
    // further PCs are built (equivalent to handle.stop()).
    controller.abort();
    const countAtAbort = liveRtcCount;
    await new Promise(r => setTimeout(r, 200));
    expect(liveRtcCount).toBe(countAtAbort);

    handle.stop();
  });

  it('escalates the PApp connection phase to error after repeated failed handshakes', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x01),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      // own (0x01) < peer (0x02) → Desktop is the initiator; the orchestrator
      // reports this peer's phase transitions to ./resource regardless of which
      // peer the consumer ultimately tracks.
      fetchInitialPeers: () =>
        Promise.resolve([
          {
            statementAccountId: new Uint8Array(32).fill(0x02),
            encryptionPublicKey: PEER_ENC_PUB,
          },
        ]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement: async () => {},
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
      handshakeTimeoutMs: 40,
      respawnBackoffMs: 10,
    });

    // The DC never opens; with a 40ms budget, well over 3 timeout cycles elapse
    // within 300ms, so the phase must cross the failure threshold into `error`.
    await new Promise(r => setTimeout(r, 300));
    handle.stop();

    const pappPeer = '0x' + '02'.repeat(32);
    const phases = phaseSpy.mock.calls.filter(c => c[0] === pappPeer).map(c => c[1]);
    // Reported as connecting while below the threshold, then escalates to error.
    expect(phases).toContain('connecting');
    expect(phases).toContain('error');
    expect(phases.indexOf('error')).toBeGreaterThan(phases.indexOf('connecting'));
    // The DC never opened, so it must never have claimed it was syncing.
    expect(phases).not.toContain('syncing');
  });

  // Regression guard for a live failure: sending `reconnected` on every respawn
  // (Android/iOS both do) stopped sync starting at all. The chat spec puts the
  // send above the reconnect loop — once per process — and that is what a peer
  // actually expects. Do not "fix" this to per-attempt again without a paired
  // device test.
  // The chat spec allows one Reconnected per process.
  it('sends Reconnected only on the first spawn, never again on respawn', async () => {
    // own (0x05) > peer (0x02) → Desktop is the ACCEPTOR, so its signaler sends
    // NOTHING on spawn — every captured post is a restart-recovery Reconnected,
    // which lets us count them without decrypting.
    const peerHex = '0x' + '02'.repeat(32);
    await deviceSyncRepository.upsert({
      statementAccountId: peerHex,
      encryptionPublicKey: '0x' + Buffer.from(PEER_ENC_PUB).toString('hex'),
      status: 'active',
      lastUpdate: 1,
      outgoingUpdateTime: 0,
      lastOfferId: 'persisted-attempt',
    });

    const postStatement = vi.fn(async () => {});
    const handle = await startDeviceSyncOrchestrator({
      ownDevice: {
        statementAccountId: new Uint8Array(32).fill(0x05),
        encryptionPrivateKey: OWN_ENC_PRIV,
        encryptionPublicKey: OWN_ENC_PUB,
      },
      fetchInitialPeers: () =>
        Promise.resolve([{ statementAccountId: new Uint8Array(32).fill(0x02), encryptionPublicKey: PEER_ENC_PUB }]),
      subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
      postStatement,
      resolveConsumerInfo: () => Promise.resolve(null),
      ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
      iceConfig: {},
      // The data channel never opens, so each timeout respawns.
      handshakeTimeoutMs: 80,
      respawnBackoffMs: 10,
    });

    await new Promise(r => setTimeout(r, 400));
    handle.stop();

    // Three spawns happened in that window; exactly one announce must have gone out.
    expect(postStatement.mock.calls.length).toBe(1);
  });
});

describe('startDeviceSyncOrchestrator — respawn backoff', () => {
  // Own id (0x01) < peer id (0x02) → initiator, so every spawn builds a peer
  // connection and `liveRtcCount` tracks respawns directly.
  const backoffParams = () => ({
    ownDevice: {
      statementAccountId: new Uint8Array(32).fill(0x01),
      encryptionPrivateKey: OWN_ENC_PRIV,
      encryptionPublicKey: OWN_ENC_PUB,
    },
    fetchInitialPeers: () =>
      Promise.resolve([{ statementAccountId: new Uint8Array(32).fill(0x02), encryptionPublicKey: PEER_ENC_PUB }]),
    subscribeStatementTopic: () => new Subject<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>().asObservable(),
    postStatement: vi.fn(async () => {}),
    resolveConsumerInfo: () => Promise.resolve(null),
    ownUserId: '5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty',
    iceConfig: {},
  });

  it('waits out the respawn backoff before rebuilding a failed handshake', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ...backoffParams(),
      handshakeTimeoutMs: 40,
      respawnBackoffMs: 200,
    });

    // 40ms timeout + 200ms backoff: at 150ms the respawn must not have happened.
    await new Promise(r => setTimeout(r, 150));
    const duringBackoff = liveRtcCount;
    await new Promise(r => setTimeout(r, 200));

    expect(duringBackoff).toBe(1);
    expect(liveRtcCount).toBe(2);
    handle.stop();
  });

  it('cancels a pending respawn when the orchestrator stops', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ...backoffParams(),
      handshakeTimeoutMs: 40,
      respawnBackoffMs: 200,
    });

    await new Promise(r => setTimeout(r, 150)); // inside the backoff window
    handle.stop();
    const atStop = liveRtcCount;
    await new Promise(r => setTimeout(r, 300));

    expect(liveRtcCount).toBe(atStop);
  });

  // Regression, and the inverse of what this test first asserted: the desktop
  // must KEEP its offerId when the data channel drops. The spec's reconnect path
  // (connectionLoop step 5) fires only when a peer's `reconnected(offerId)`
  // matches our active attempt — "if signaling.activeOfferId == offerIdToDispose"
  // — so minting a fresh attempt on channel close throws away the id the
  // returning peer will name, and it can never reconnect. Transport death is the
  // `connectionState` watch's job; peer restart is `reconnected`'s.
  it('holds the attempt when the data channel closes, so a returning peer can dispose it by offerId', async () => {
    const handle = await startDeviceSyncOrchestrator({
      ...backoffParams(),
      handshakeTimeoutMs: 10_000, // long: this test must not respawn via timeout
      respawnBackoffMs: 30,
    });

    await new Promise(r => setTimeout(r, 50));
    lastDataChannel!.readyState = 'open';
    lastDataChannel!.fire('open');
    await new Promise(r => setTimeout(r, 30));
    expect(liveRtcCount).toBe(1);

    // The phone goes away: channel closes, peer connection stays 'connected'.
    lastDataChannel!.readyState = 'closed';
    lastDataChannel!.fire('close');
    await new Promise(r => setTimeout(r, 200));

    expect(liveRtcCount).toBe(1);
    handle.stop();
  });
});
