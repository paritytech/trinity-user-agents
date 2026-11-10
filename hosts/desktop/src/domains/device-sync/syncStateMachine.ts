/**
 * Per-pair sync state machine: collect → send SyncUpdate → wait Ack, plus
 * inbound apply + Ack-emit. Reset on each fresh data channel; the durable
 * cursor lives in `outgoingUpdateTime` via caller-supplied advance/get fns.
 */

import { toHex } from 'polkadot-api/utils';
import { type CodecType } from 'scale-ts';

import { DEVICE_SYNC_USE_CASE_ID, DataChannelMessageCodec } from '@/shared/peer-channel';

import { type CollectedChanges } from './collector';
import { type SyncEntityCodec, SyncMessageCodec } from './schemas';
import { type SyncUpdateChunk, deviceSyncService } from './service';
import { type SyncActivity } from './types';

type SyncEntity = CodecType<typeof SyncEntityCodec>;
type SyncMessage = CodecType<typeof SyncMessageCodec>;

// How long to wait for an Ack before clearing `inflight` and resending the Update.
// Mirrors Android (DeviceSyncRunner.kt: withTimeoutOrNull(30.seconds)). Treats the
// symptom of a lost Update — the root cause (Update not reaching the peer) is separate.
const ACK_TIMEOUT_MS = 30_000;

export type SyncStateMachineParams = {
  peerStatementAccountId: string;
  dataChannel: RTCDataChannel;
  collect: () => Promise<CollectedChanges>;
  apply: (entities: SyncEntity[]) => Promise<void>;
  getOutgoingUpdateTime: () => Promise<number>;
  advanceOutgoingUpdateTime: (peerId: string, timePoint: number) => Promise<void>;
  /** Ack wait before resending the in-flight Update. Defaults to {@link ACK_TIMEOUT_MS}. */
  ackTimeoutMs?: number;
  /**
   * Reports the machine's own activity: `'active'` once an Update is in flight,
   * `'idle'` when the outbound queue drains, `'error'` on a non-recoverable
   * collect/apply failure. Connectivity is the caller's concern, not this enum's.
   */
  onActivityChange?: (activity: SyncActivity) => void;
};

export type SyncStateMachineHandle = {
  poke: () => void;
  close: () => void;
};

export function startSyncStateMachine(params: SyncStateMachineParams): SyncStateMachineHandle {
  const { dataChannel, collect, apply, advanceOutgoingUpdateTime } = params;
  const ackTimeoutMs = params.ackTimeoutMs ?? ACK_TIMEOUT_MS;
  let nextId = 1;
  let inflight: { id: number; timePoint: number } | null = null;
  // Remaining chunks of the current round. A snapshot too large for one
  // `SyncUpdate` is split (spec: split when it would exceed the data channel's
  // max-message-size). Each chunk is its own id/Ack round and carries its own
  // checkpoint, so every acknowledgement makes durable progress — a backlog that
  // outlives one connection still drains monotonically instead of restarting.
  let pendingChunks: SyncUpdateChunk[] = [];
  let ackTimer: ReturnType<typeof setTimeout> | null = null;
  let lastAppliedInboundId = 0;
  let closed = false;
  let idleNotified = false;

  const notifyIdle = (): void => {
    if (closed || idleNotified || inflight) return;
    idleNotified = true;
    params.onActivityChange?.('idle');
  };

  /** Returns whether the message actually left the machine. */
  function send(sync: SyncMessage): boolean {
    // Every path into send() crosses an await first — `pump` after `collect()`,
    // both Ack paths after `apply()` — and the data channel can close during it
    // (handshake-timeout respawn, connection failure, orchestrator stop). The
    // entry-level `closed` checks are therefore not enough: they are evaluated
    // before the await, not after. Guarding here covers every caller at once.
    //
    // Dropping is the correct response, not an error: an unsent Update is
    // re-collected by the next pump (the cursor only advances on Ack), and an
    // unsent Ack makes the peer resend its Update after its own ack timeout.
    if (closed || dataChannel.readyState !== 'open') {
      console.debug('WEBRTC [sync] dropping %s — data channel is %s', sync.tag, dataChannel.readyState);

      return false;
    }

    const data = SyncMessageCodec.enc(sync);
    const envelope = DataChannelMessageCodec.enc({
      id: DEVICE_SYNC_USE_CASE_ID,
      data,
    });
    const buffer = new ArrayBuffer(envelope.byteLength);
    new Uint8Array(buffer).set(envelope);
    try {
      dataChannel.send(buffer);

      return true;
    } catch (err) {
      // Still reachable with an open channel — a full send buffer throws here.
      console.error('WEBRTC [sync] dataChannel.send failed: %s', err instanceof Error ? err.message : String(err));

      return false;
    }
  }

  function clearAckTimer(): void {
    if (ackTimer !== null) {
      clearTimeout(ackTimer);
      ackTimer = null;
    }
  }

  function armAckTimer(): void {
    clearAckTimer();
    ackTimer = setTimeout(onAckTimeout, ackTimeoutMs);
  }

  function onAckTimeout(): void {
    ackTimer = null;
    if (closed || !inflight) return;
    console.warn(
      'WEBRTC [sync] no Ack for Update id=%d within %dms peer=%s — clearing inflight and resending',
      inflight.id,
      ackTimeoutMs,
      params.peerStatementAccountId,
    );
    // Do NOT advance outgoingUpdateTime: with the cursor unchanged, the resend collects the
    // same (or a superset of) changes and ships them with a fresh id, exactly like Android.
    inflight = null;
    void pump();
  }

  async function pump(): Promise<void> {
    if (closed) return;
    if (inflight) return;

    if (pendingChunks.length === 0) {
      let changes: CollectedChanges;
      try {
        changes = await collect();
      } catch (err) {
        console.error('WEBRTC [sync] collect() failed: %s', err instanceof Error ? err.message : String(err));
        params.onActivityChange?.('error');
        return;
      }
      if (changes.entities.length === 0) {
        notifyIdle();
        return;
      }
      pendingChunks = deviceSyncService.chunkSyncEntities(changes.entities, changes.timePoint);
      if (pendingChunks.length > 1) {
        console.info('WEBRTC [sync] snapshot split into %d updates peer=%s', pendingChunks.length, params.peerStatementAccountId);
      }
    }

    const chunk = pendingChunks[0]!;
    const id = nextId++;
    const sent = send({
      tag: 'Update',
      value: { id, entities: chunk.entities, timePoint: BigInt(chunk.timePoint) },
    });
    // Commit state only for an Update that actually went out. Recording a
    // phantom `inflight` would report `active` for a machine that sent nothing
    // and arm an ack timer whose 30s expiry logs a lost-Update warning for a
    // message that never existed. The cursor has not advanced, so the next pump
    // simply re-collects the same changes.
    if (!sent) return;
    idleNotified = false;
    inflight = { id, timePoint: chunk.timePoint };
    params.onActivityChange?.('active');
    armAckTimer();
  }

  async function onMessage(ev: MessageEvent<ArrayBuffer | Uint8Array>): Promise<void> {
    const bytes = ev.data instanceof Uint8Array ? ev.data : new Uint8Array(ev.data);
    let envelope;
    try {
      envelope = DataChannelMessageCodec.dec(bytes);
    } catch (err) {
      console.warn(
        'WEBRTC [sync] envelope decode failed dataLen=%d firstBytes=%s err=%s',
        bytes.length,
        toHex(bytes.slice(0, Math.min(64, bytes.length))),
        err instanceof Error ? err.message : String(err),
      );
      return;
    }
    if (envelope.id !== DEVICE_SYNC_USE_CASE_ID) {
      return;
    }
    let sync: SyncMessage;
    try {
      sync = SyncMessageCodec.dec(envelope.data);
    } catch (err) {
      // Hex prefix + tail let us pinpoint which entity blew up the decoder.
      console.warn(
        'WEBRTC [sync] SyncMessage decode failed dataLen=%d firstBytes=%s err=%s',
        envelope.data.length,
        toHex(envelope.data.slice(0, Math.min(64, envelope.data.length))),
        err instanceof Error ? err.message : String(err),
      );
      if (envelope.data.length > 64) {
        console.warn(
          'WEBRTC [sync] SyncMessage decode failed lastBytes=%s',
          toHex(envelope.data.slice(Math.max(0, envelope.data.length - 32))),
        );
      }
      console.warn('WEBRTC [sync] SyncMessage decode failed fullBytes=%s', toHex(envelope.data));
      return;
    }
    if (sync.tag === 'Update') {
      if (sync.value.id <= lastAppliedInboundId) {
        send({ tag: 'Ack', value: { id: sync.value.id } });
        return;
      }
      try {
        await apply(sync.value.entities);
      } catch (err) {
        console.error('WEBRTC [sync] apply() failed: %s', err instanceof Error ? err.message : String(err));
        params.onActivityChange?.('error');
        return; // don't Ack — peer will retry
      }
      lastAppliedInboundId = sync.value.id;
      send({ tag: 'Ack', value: { id: sync.value.id } });
    } else if (sync.tag === 'Ack') {
      if (!inflight || inflight.id !== sync.value.id) return;
      clearAckTimer();
      const advancedTo = inflight.timePoint;
      inflight = null;
      // This chunk is delivered; drop it so the next pump ships the following one.
      pendingChunks.shift();
      // Every Ack advances: the chunk's checkpoint covers only what it fully
      // delivered, and the repository ignores a checkpoint that is not ahead.
      try {
        await advanceOutgoingUpdateTime(params.peerStatementAccountId, advancedTo);
      } catch (err) {
        console.error('WEBRTC [sync] advanceOutgoingUpdateTime failed: %s', err instanceof Error ? err.message : String(err));
      }
      void pump();
      notifyIdle();
    }
  }

  function listener(ev: MessageEvent): void {
    void onMessage(ev);
  }
  dataChannel.addEventListener('message', listener);

  const handle: SyncStateMachineHandle = {
    poke: () => void pump(),
    close: () => {
      closed = true;
      clearAckTimer();
      dataChannel.removeEventListener('message', listener);
      dataChannel.removeEventListener('close', onChannelClosed);
    },
  };

  // The data channel IS this machine's transport: when it closes the machine is
  // done. Without this the machine outlives its channel until the orchestrator's
  // teardown happens to run — ack timer still counting down, `poke()` from the
  // local-change signal still pumping into a dead socket. close() is idempotent,
  // so the orchestrator's own teardown remains safe.
  function onChannelClosed(): void {
    handle.close();
  }
  dataChannel.addEventListener('close', onChannelClosed);

  void pump();

  return handle;
}
