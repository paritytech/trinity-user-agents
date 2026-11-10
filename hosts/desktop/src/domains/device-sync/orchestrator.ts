/**
 * Per-pair WebRTC sync orchestration. Seeds the authorising PApp peer from
 * `fetchInitialPeers()`, then for each active peer spawns a signaler + peer
 * connection + sync state machine. Tears down on `stop()`; respawns on PC
 * `disconnected`/`failed`. PApp is authoritative for the device roster — it
 * arrives over the first `SyncUpdate.Devices` and lands via the applier.
 */

import { fromHex, toHex } from 'polkadot-api/utils';
import { type Observable, auditTime } from 'rxjs';

import { type IceConfigParams, createPeerConnection } from '@/shared/peer-channel';
import { deviceIdentityService } from '@/domains/device';
import { deviceSessionUseCase } from '@/domains/device-session';

import { type ConsumerInfoLookup, applySyncEntities } from './applier';
import { collectChangesSince } from './collector';
import { DEVICE_SYNC_HANDSHAKE_TIMEOUT_MS, DEVICE_SYNC_MAX_FAILED_HANDSHAKES, DEVICE_SYNC_RESPAWN_BACKOFF_MS } from './constants';
import { localSyncSignal$ } from './localChangeSignal';
import { deviceSyncRepository } from './repository';
import { reportPeerSyncPhase } from './resource';
import { startSignaler } from './signaler';
import { type SyncStateMachineHandle, startSyncStateMachine } from './syncStateMachine';
import { type KnownUserDevice } from './types';

export type DeviceSyncOrchestratorParams = {
  ownDevice: {
    statementAccountId: Uint8Array;
    encryptionPrivateKey: Uint8Array;
    encryptionPublicKey: Uint8Array;
  };
  fetchInitialPeers: () => Promise<{ statementAccountId: Uint8Array; encryptionPublicKey: Uint8Array }[]>;
  subscribeStatementTopic: (topic: Uint8Array) => Observable<{ topic: Uint8Array; data: Uint8Array; signer: Uint8Array }>;
  postStatement: (topic: Uint8Array, data: Uint8Array, channel: Uint8Array) => Promise<void>;
  resolveConsumerInfo: ConsumerInfoLookup;
  /** SS58 of the device statement account (= `session.localAccount.accountId` for V2 sessions).
   * Written into `P2PRoom.userId` so the synced room is found by the chat-list hook,
   * which queries rooms by this same SS58. */
  ownUserId: string;
  iceConfig: IceConfigParams;
  /** Handshake budget before a stalled connection is torn down and rebuilt.
   * Defaults to {@link DEVICE_SYNC_HANDSHAKE_TIMEOUT_MS}; overridable for tests. */
  handshakeTimeoutMs?: number;
  /** Pause before a torn-down attempt is rebuilt.
   * Defaults to {@link DEVICE_SYNC_RESPAWN_BACKOFF_MS}; overridable for tests. */
  respawnBackoffMs?: number;
  /** Aborts a superseded start: stops spawning and tears down everything spawned so
   * far, so an orchestrator the caller has already discarded never keeps submitting. */
  signal?: AbortSignal;
};

export type DeviceSyncOrchestratorHandle = {
  stop: () => void;
};

/** Lexicographic byte compare: the peer with the smaller statement account id is the initiator. */
function isLessThan(a: Uint8Array, b: Uint8Array): boolean {
  const len = Math.min(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const av = a[i] ?? 0;
    const bv = b[i] ?? 0;
    if (av < bv) return true;
    if (av > bv) return false;
  }
  return a.length < b.length;
}

export async function startDeviceSyncOrchestrator(params: DeviceSyncOrchestratorParams): Promise<DeviceSyncOrchestratorHandle> {
  const ownStmtHex = toHex(params.ownDevice.statementAccountId);
  const handshakeTimeoutMs = params.handshakeTimeoutMs ?? DEVICE_SYNC_HANDSHAKE_TIMEOUT_MS;
  const respawnBackoffMs = params.respawnBackoffMs ?? DEVICE_SYNC_RESPAWN_BACKOFF_MS;
  const closers: (() => void)[] = [];

  // Consecutive failed handshakes per peer. Reset on a successful data channel;
  // once a peer reaches DEVICE_SYNC_MAX_FAILED_HANDSHAKES its reported phase holds
  // at `error` (not terminal — a later DC open recovers it). The respawn loop
  // keeps running underneath; this only governs the phase the engine reports.
  const consecutiveHandshakeFailures = new Map<string, number>();
  const reportAttemptPhase = (peerId: string): void => {
    const failures = consecutiveHandshakeFailures.get(peerId) ?? 0;
    reportPeerSyncPhase(peerId, failures >= DEVICE_SYNC_MAX_FAILED_HANDSHAKES ? 'error' : 'connecting');
  };

  // At most one signaler per peer for the orchestrator's lifetime.
  const activePeerSignalers = new Set<string>();

  // Respawns waiting out their backoff. Cleared on stop() so a torn-down
  // orchestrator cannot resurrect itself after the delay.
  const pendingRespawns = new Set<ReturnType<typeof setTimeout>>();
  closers.push(() => {
    for (const timer of pendingRespawns) clearTimeout(timer);
    pendingRespawns.clear();
  });

  // Peers already told to dispose a prior attempt. Per the chat spec
  // (`connectionLoop`), `reconnected` is sent ONCE per process start — the send
  // sits above the reconnect loop, not inside it — so a handshake-timeout
  // respawn must NOT re-send it.
  //
  // This was briefly changed to per-attempt to match Android
  // (`connectOnce -> sendReconnectIfResuming`) and iOS (`startConnection`), both
  // of which send it every attempt. Live testing showed sync then failing to
  // start at all, so the spec's placement is authoritative and the mobile
  // clients' extra sends are what a peer tolerates, not what it expects.
  const reconnectSignalSent = new Set<string>();

  // Active state machines keyed by peer; poked on local writes so changes ship
  // immediately rather than waiting for the next DC reconnect.
  const activeStateMachines = new Map<string, SyncStateMachineHandle>();
  const localChangeSub = localSyncSignal$.pipe(auditTime(50)).subscribe(() => {
    for (const sm of activeStateMachines.values()) sm.poke();
  });
  closers.push(() => localChangeSub.unsubscribe());

  function spawn(peer: KnownUserDevice): void {
    if (activePeerSignalers.has(peer.statementAccountId)) return;
    activePeerSignalers.add(peer.statementAccountId);

    reportAttemptPhase(peer.statementAccountId);

    const peerStmtBytes = fromHex(peer.statementAccountId);
    const peerEncBytes = fromHex(peer.encryptionPublicKey);
    const role: 'initiator' | 'acceptor' = isLessThan(params.ownDevice.statementAccountId, peerStmtBytes)
      ? 'initiator'
      : 'acceptor';

    const session = deviceSessionUseCase.createChannel({
      ourDeviceEncPriv: params.ownDevice.encryptionPrivateKey,
      ourStatementAccountId: params.ownDevice.statementAccountId,
      peerDeviceEncPub: peerEncBytes,
      peerStatementAccountId: peerStmtBytes,
      post: params.postStatement,
      subscribe: params.subscribeStatementTopic,
    });

    const peerConn = createPeerConnection({
      role,
      dataChannelLabel: 'sync',
      iceConfig: params.iceConfig,
    });

    // Restart recovery: on the FIRST spawn after process start, if a previous
    // attempt's offerId is persisted, the peer is told to dispose it so both
    // ends start clean instead of waiting out a stale-handshake timeout. Once
    // per process, per the spec's `connectionLoop` — see `reconnectSignalSent`.
    //
    // The signaler does the sending, not us: an initiator must put the
    // Reconnected in the SAME statement as its fresh Offer, and only the
    // signaler knows when that Offer is ready.
    const reconnectOfferId = reconnectSignalSent.has(peer.statementAccountId) ? undefined : peer.lastOfferId;
    reconnectSignalSent.add(peer.statementAccountId);

    let respawning = false;
    // Whether THIS spawn's data channel ever opened. Distinguishes a handshake
    // that never connected (counts toward the error threshold) from a working
    // session that later dropped (restarts the count from zero).
    let dcOpened = false;
    const signaler = startSignaler({
      session,
      peerConnection: peerConn,
      role,
      reconnectOfferId,
      onAcceptedOfferId: offerId => {
        void deviceSyncRepository.setLastOfferId(peer.statementAccountId, offerId);
      },
      onResetRequest: () => {
        // Peer asked us (via a matching Reconnected) to dispose this attempt.
        // Clear the persisted offerId (it's dead) and respawn a fresh attempt.
        //
        // Immediately, with no backoff: the spec's connection loop disposes and
        // calls `createAndStartPeer` right there, and the peer has explicitly
        // asked us to rebuild now. The backoff exists to stop us hammering a peer
        // that is simply unreachable — a reset is the opposite situation.
        void deviceSyncRepository.setLastOfferId(peer.statementAccountId, null);
        respawnPeer('failed', { immediate: true });
      },
    });

    // Per-spawn teardown bag — keeps a PC-death respawn scoped to this peer.
    const spawnClosers: (() => void)[] = [];

    // Android parity (`awaitOpenOrTerminal`): retry on open OR terminal OR
    // timeout. The connectionState$ watch below covers terminal; this covers
    // "stuck in connecting/new" — cleared the moment the DC opens.
    const handshakeTimer = setTimeout(() => {
      console.warn(
        'WEBRTC [orchestrator] handshake timeout (%dms) peer=%s connection=%s — respawning',
        handshakeTimeoutMs,
        peer.statementAccountId,
        peerConn.connectionState(),
      );
      respawnPeer('failed');
    }, handshakeTimeoutMs);
    spawnClosers.push(() => clearTimeout(handshakeTimer));

    const dataChannelSub = signaler.dataChannel$.subscribe({
      next: channel => {
        clearTimeout(handshakeTimer);
        dcOpened = true;
        consecutiveHandshakeFailures.set(peer.statementAccountId, 0);
        reportPeerSyncPhase(peer.statementAccountId, 'syncing');
        const sm = startSyncStateMachine({
          peerStatementAccountId: peer.statementAccountId,
          dataChannel: channel,
          // Re-read checkpoint from the repository on every pump. Closing over
          // `peer.outgoingUpdateTime` would freeze it at spawn-time and cause
          // an echo loop (Update id=1..N with identical payload).
          collect: async () => {
            const row = await deviceSyncRepository.get(peer.statementAccountId);
            const since = row?.outgoingUpdateTime ?? 0;
            return collectChangesSince(since);
          },
          apply: entities =>
            applySyncEntities(entities, {
              resolveConsumerInfo: params.resolveConsumerInfo,
              ownUserId: params.ownUserId,
            }),
          getOutgoingUpdateTime: async () => {
            const row = await deviceSyncRepository.get(peer.statementAccountId);
            return row?.outgoingUpdateTime ?? 0;
          },
          advanceOutgoingUpdateTime: deviceSyncRepository.advanceOutgoingUpdateTime,
          onActivityChange: activity => {
            if (activity === 'active') reportPeerSyncPhase(peer.statementAccountId, 'syncing');
            else if (activity === 'idle') reportPeerSyncPhase(peer.statementAccountId, 'synced');
            else reportPeerSyncPhase(peer.statementAccountId, 'error');
          },
        });
        activeStateMachines.set(peer.statementAccountId, sm);
        spawnClosers.push(() => {
          activeStateMachines.delete(peer.statementAccountId);
          sm.close();
        });

        // NOTE: a closing data channel deliberately does NOT respawn here. The
        // spec's reconnect path (connectionLoop step 5) is the peer sending
        // `reconnected(offerId)` on ITS restart, and it only fires when we still
        // hold that offerId: "if signaling.activeOfferId == offerIdToDispose".
        // Minting a fresh attempt the moment the channel drops throws away the
        // very id the returning peer will name, so its `reconnected` no longer
        // matches and the mechanism is defeated. Transport death is covered by
        // the `connectionState` watch below; peer restart is covered by
        // `onResetRequest`.
      },
    });
    spawnClosers.push(() => dataChannelSub.unsubscribe());
    spawnClosers.push(() => signaler.close());

    // Respawn the PC on `disconnected`/`failed`. WebRTC reuses the PC on a
    // peer's re-handshake but does NOT re-emit `datachannel` for the peer's
    // fresh DC, so the only reliable recovery is full teardown + rebuild.
    // `disconnected` is sometimes transient — wait 2s; `failed` is terminal.
    let deathTimer: ReturnType<typeof setTimeout> | null = null;
    const respawnSub = peerConn.connectionState$.subscribe({
      next: state => {
        if (state === 'failed') {
          if (deathTimer !== null) {
            clearTimeout(deathTimer);
            deathTimer = null;
          }
          reportPeerSyncPhase(peer.statementAccountId, 'disconnected');
          console.warn('WEBRTC [orchestrator] connection FAILED peer=%s — respawning', peer.statementAccountId);
          respawnPeer('failed');
        } else if (state === 'disconnected') {
          if (deathTimer !== null) return;
          console.warn(
            'WEBRTC [orchestrator] connection DISCONNECTED peer=%s — 2s grace before respawn',
            peer.statementAccountId,
          );
          deathTimer = setTimeout(() => {
            deathTimer = null;
            const cur = peerConn.connectionState();
            if (cur === 'disconnected' || cur === 'failed') {
              reportPeerSyncPhase(peer.statementAccountId, 'disconnected');
              console.warn('WEBRTC [orchestrator] still %s after grace peer=%s — respawning', cur, peer.statementAccountId);
              respawnPeer(cur);
            }
          }, 2000);
        } else if (state === 'connected') {
          if (deathTimer !== null) {
            clearTimeout(deathTimer);
            deathTimer = null;
          }
        }
      },
    });
    spawnClosers.push(() => respawnSub.unsubscribe());
    spawnClosers.push(() => {
      if (deathTimer !== null) {
        clearTimeout(deathTimer);
        deathTimer = null;
      }
    });

    function respawnPeer(_cause: RTCPeerConnectionState, options?: { immediate?: boolean }): void {
      if (respawning) return;
      respawning = true;
      for (const c of spawnClosers) {
        try {
          c();
        } catch (e) {
          console.warn('WEBRTC [orchestrator] spawn closer threw: %s', e instanceof Error ? e.message : String(e));
        }
      }
      // A working session that dropped restarts the count; a handshake that
      // never opened its DC counts as a failure. The re-spawn's start then
      // reports `error` once the count crosses the threshold.
      const priorFailures = consecutiveHandshakeFailures.get(peer.statementAccountId) ?? 0;
      consecutiveHandshakeFailures.set(peer.statementAccountId, dcOpened ? 0 : priorFailures + 1);
      // Android parity (`DeviceSyncEngine.RECONNECT_BACKOFF`): pause before
      // rebuilding rather than immediately re-offering, so a peer that is simply
      // unreachable is not hammered with a fresh attempt per failure.
      //
      // The peer stays in `activePeerSignalers` for the whole backoff and is only
      // released as the respawn runs: a stop() landing mid-backoff must still
      // report the peer `inactive`, and that loop reads this set.
      const timer = setTimeout(
        () => {
          pendingRespawns.delete(timer);
          activePeerSignalers.delete(peer.statementAccountId);
          spawn(peer);
        },
        options?.immediate ? 0 : respawnBackoffMs,
      );
      pendingRespawns.add(timer);
    }

    closers.push(() => {
      for (const c of spawnClosers) c();
    });
  }

  const initial = await params.fetchInitialPeers();
  const now = Date.now();
  for (const peer of initial) {
    if (!deviceIdentityService.isValidEncryptionPublicKey(peer.encryptionPublicKey)) {
      console.warn(
        'WEBRTC [orchestrator] skipping seed peer=%s — encryptionPublicKey is not a valid X25519 key (len=%d)',
        toHex(peer.statementAccountId),
        peer.encryptionPublicKey.length,
      );
      continue;
    }
    const stmtHex = toHex(peer.statementAccountId);
    const existing = await deviceSyncRepository.get(stmtHex);
    if (!existing) {
      await deviceSyncRepository.upsert({
        statementAccountId: stmtHex,
        encryptionPublicKey: toHex(peer.encryptionPublicKey),
        status: 'active',
        lastUpdate: now,
        outgoingUpdateTime: 0,
      });
    }
  }

  const allPeers = await deviceSyncRepository.listActivePeers(ownStmtHex);
  // Purge rows whose enc key can't serve ECDH — earlier builds persisted
  // host-papp 0.8.6's SSO shared secret as a "key" here, and the chat manager
  // fans these rows out to peers as `deviceAdded` (poisoning THEIR sends too).
  const peers: KnownUserDevice[] = [];
  for (const peer of allPeers) {
    if (deviceIdentityService.isValidEncryptionPublicKey(fromHex(peer.encryptionPublicKey))) {
      peers.push(peer);
      continue;
    }
    console.warn(
      'WEBRTC [orchestrator] purging persisted peer=%s — encryptionPublicKey is not a valid X25519 key',
      peer.statementAccountId,
    );
    await deviceSyncRepository.remove(peer.statementAccountId);
  }
  // Stagger initial spawns. Each signaler synchronously posts its first
  // SDP/ICE bundle to the statement-store at construction time; firing them
  // in parallel saturates the account's per-block submission budget and the
  // chain returns `AccountFullError`, forcing exponential backoff. A small
  // delay between spawns lets the budget recover between Offers.
  let stopped = false;
  const stop = (): void => {
    if (stopped) return;
    stopped = true;
    for (const c of closers) c();
    for (const peerId of activePeerSignalers) reportPeerSyncPhase(peerId, 'inactive');
  };
  // A start superseded by a newer identity is aborted mid-flight: stop spawning
  // and tear down whatever already spawned, so a discarded orchestrator never
  // keeps submitting (two live orchestrators on the same account collide on
  // identical second-resolution expiries). `once` + idempotent `stop` make a
  // later caller-driven stop a no-op.
  params.signal?.addEventListener('abort', stop, { once: true });
  // Aborted during the awaits above (before the listener was attached): the
  // listener won't fire retroactively, so bail explicitly.
  if (params.signal?.aborted) {
    stop();
    return { stop };
  }

  const SPAWN_INTERVAL_MS = 600;
  for (let i = 0; i < peers.length; i++) {
    if (params.signal?.aborted) break;
    // One bad peer must not take down sync with every other device: spawn()
    // synchronously derives ECDH topics and can throw on corrupt key material.
    try {
      spawn(peers[i]!);
    } catch (e) {
      console.warn(
        'WEBRTC [orchestrator] spawn failed peer=%s — skipping: %s',
        peers[i]!.statementAccountId,
        e instanceof Error ? e.message : String(e),
      );
      activePeerSignalers.delete(peers[i]!.statementAccountId);
    }
    if (i < peers.length - 1) {
      await new Promise<void>(resolve => setTimeout(resolve, SPAWN_INTERVAL_MS));
    }
  }

  return { stop };
}
