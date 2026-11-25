import { Subject } from 'rxjs';
import { describe, expect, it } from 'vitest';

import { DEVICE_SYNC_USE_CASE_ID, DataChannelMessageCodec } from '@/shared/peer-channel';

import { SyncMessageCodec } from './schemas';
import { startSyncStateMachine } from './syncStateMachine';

function makeFakeChannel() {
  const sent: Uint8Array[] = [];
  const closeListeners: (() => void)[] = [];
  const messages$ = new Subject<MessageEvent<ArrayBuffer | Uint8Array>>();
  /* eslint-disable @typescript-eslint/consistent-type-assertions -- test mock channel */
  const channel = {
    readyState: 'open' as RTCDataChannelState,
    send: (data: Uint8Array) => sent.push(data),
    addEventListener: (ev: string, cb: (e: MessageEvent<ArrayBuffer | Uint8Array>) => void) => {
      if (ev === 'message') messages$.subscribe({ next: e => cb(e) });
      if (ev === 'close') closeListeners.push(cb as unknown as () => void);
    },
    removeEventListener: () => {},
  } as unknown as RTCDataChannel;
  /* eslint-enable @typescript-eslint/consistent-type-assertions */
  const setReadyState = (state: RTCDataChannelState) => {
    /* eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test mock channel */
    (channel as unknown as { readyState: RTCDataChannelState }).readyState = state;
  };

  /** Drives the channel's own `close` event, as WebRTC does on teardown. */
  const fireClose = () => {
    setReadyState('closed');
    for (const cb of closeListeners) cb();
  };

  return { channel, sent, messages$, setReadyState, fireClose };
}

const CHANGES = {
  entities: [
    {
      entity: { tag: 'ChatsAdded' as const, value: [{ tag: 'Contact' as const, value: new Uint8Array(32).fill(0xab) }] },
      maxTimestamp: 999,
    },
  ],
  timePoint: 999,
};

function delay(ms: number): Promise<void> {
  return new Promise(r => setTimeout(r, ms));
}

/** Deliver a SyncUpdateAck over the fake channel, the way the peer would. */
function ack(messages$: Subject<MessageEvent<ArrayBuffer | Uint8Array>>, id: number): void {
  const bytes = DataChannelMessageCodec.enc({
    id: DEVICE_SYNC_USE_CASE_ID,
    data: SyncMessageCodec.enc({ tag: 'Ack', value: { id } }),
  });
  /* eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test mock event */
  messages$.next({ data: bytes.buffer } as MessageEvent<ArrayBuffer>);
}

function decodeUpdateId(bytes: Uint8Array): number {
  const sync = SyncMessageCodec.dec(DataChannelMessageCodec.dec(bytes).data);
  if (sync.tag !== 'Update') throw new Error('expected Update');
  return sync.value.id;
}

describe('startSyncStateMachine', () => {
  it('on first pump with empty changes, sends nothing', async () => {
    const { channel, sent } = makeFakeChannel();

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => ({ entities: [], timePoint: 0 }),
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async () => {},
    });

    await delay(10);
    expect(sent).toHaveLength(0);
    handle.close();
  });

  it('sends SyncUpdate when collector returns entities, advances on Ack', async () => {
    const { channel, sent, messages$ } = makeFakeChannel();
    let advanced = 0;

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => ({
        entities: [
          {
            entity: { tag: 'ChatsAdded' as const, value: [{ tag: 'Contact' as const, value: new Uint8Array(32).fill(0xab) }] },
            maxTimestamp: 999,
          },
        ],
        timePoint: 999,
      }),
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async (_id, t) => {
        advanced = t;
      },
    });

    await delay(10);
    expect(sent).toHaveLength(1);

    const env = DataChannelMessageCodec.dec(sent[0]!);
    const sync = SyncMessageCodec.dec(env.data);
    expect(sync.tag).toBe('Update');
    if (sync.tag !== 'Update') throw new Error('unreachable');
    const sentId = sync.value.id;

    const ackBytes = DataChannelMessageCodec.enc({
      id: DEVICE_SYNC_USE_CASE_ID,
      data: SyncMessageCodec.enc({ tag: 'Ack', value: { id: sentId } }),
    });
    /* eslint-disable @typescript-eslint/consistent-type-assertions -- test mock event */
    messages$.next({ data: ackBytes.buffer } as MessageEvent<ArrayBuffer>);
    /* eslint-enable @typescript-eslint/consistent-type-assertions */

    await delay(10);
    expect(advanced).toBe(999);
    handle.close();
  });

  it('resends Update with a fresh id and does not advance when no Ack arrives within the timeout', async () => {
    const { channel, sent } = makeFakeChannel();
    let advanced = 0;

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => ({
        entities: [
          {
            entity: { tag: 'ChatsAdded' as const, value: [{ tag: 'Contact' as const, value: new Uint8Array(32).fill(0xab) }] },
            maxTimestamp: 999,
          },
        ],
        timePoint: 999,
      }),
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async (_id, t) => {
        advanced = t;
      },
      ackTimeoutMs: 20,
    });

    await delay(10);
    expect(sent).toHaveLength(1);
    const firstId = decodeUpdateId(sent[0]!);

    // No Ack — after the timeout the Update is re-pumped with a new id.
    await delay(40);
    expect(sent.length).toBeGreaterThanOrEqual(2);
    const secondId = decodeUpdateId(sent[1]!);
    expect(secondId).not.toBe(firstId);
    // Cursor never advances without an Ack.
    expect(advanced).toBe(0);
    handle.close();
  });

  it('a matching Ack before the timeout cancels the resend', async () => {
    const { channel, sent, messages$ } = makeFakeChannel();
    // First batch has changes; after it is acked the cursor advances, so collect() is empty.
    let collected = false;

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => {
        if (collected) return { entities: [], timePoint: 999 };
        collected = true;
        return {
          entities: [
            {
              entity: { tag: 'ChatsAdded' as const, value: [{ tag: 'Contact' as const, value: new Uint8Array(32).fill(0xab) }] },
              maxTimestamp: 999,
            },
          ],
          timePoint: 999,
        };
      },
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async () => {},
      ackTimeoutMs: 50,
    });

    await delay(10);
    expect(sent).toHaveLength(1);
    const sentId = decodeUpdateId(sent[0]!);

    const ackBytes = DataChannelMessageCodec.enc({
      id: DEVICE_SYNC_USE_CASE_ID,
      data: SyncMessageCodec.enc({ tag: 'Ack', value: { id: sentId } }),
    });
    /* eslint-disable @typescript-eslint/consistent-type-assertions -- test mock event */
    messages$.next({ data: ackBytes.buffer } as MessageEvent<ArrayBuffer>);
    /* eslint-enable @typescript-eslint/consistent-type-assertions */

    // collect() now yields nothing, so the post-Ack pump sends no new Update and the
    // timer must not fire a resend either.
    await delay(80);
    expect(sent).toHaveLength(1);
    handle.close();
  });

  // Regression: the channel can close while `collect()` is still awaiting — a
  // handshake-timeout respawn or connection failure runs independently of the
  // pump. Before the readyState guard this reached `dataChannel.send()` and threw
  // "RTCDataChannel.readyState is not 'open'".
  it('drops an Update when the data channel closed while collect() was in flight', async () => {
    const { channel, sent, setReadyState } = makeFakeChannel();

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => {
        setReadyState('closed');

        return {
          entities: [
            {
              entity: { tag: 'ChatsAdded' as const, value: [{ tag: 'Contact' as const, value: new Uint8Array(32).fill(0xab) }] },
              maxTimestamp: 999,
            },
          ],
          timePoint: 999,
        };
      },
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async () => {},
    });

    await delay(10);
    expect(sent).toHaveLength(0);
    handle.close();
  });

  it('does not report `active` or arm an ack timer for an Update that was never sent', async () => {
    const { channel, sent, setReadyState } = makeFakeChannel();
    const activities: string[] = [];

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => {
        setReadyState('closed');

        return CHANGES;
      },
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async () => {},
      ackTimeoutMs: 20,
      onActivityChange: a => activities.push(a),
    });

    // Past the ack timeout: a phantom inflight would have fired the resend here.
    await delay(60);

    expect(sent).toHaveLength(0);
    expect(activities).not.toContain('active');
    handle.close();
  });

  // The readyState guard alone would still swallow the send, so this asserts the
  // thing only self-close changes: the pending ack timer must not fire a resend
  // cycle (warning + re-collect) against a channel that is already gone.
  it('closes itself when the data channel closes, so the ack timer stops instead of resending', async () => {
    const { channel, sent, fireClose } = makeFakeChannel();
    let collectCalls = 0;

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => {
        collectCalls++;

        return CHANGES;
      },
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async () => {},
      ackTimeoutMs: 20,
    });

    await delay(10);
    expect(sent).toHaveLength(1); // first pump went out over the open channel
    expect(collectCalls).toBe(1);

    fireClose();
    await delay(60); // well past the ack timeout

    // Without self-close the timer fires, logs a lost-Update warning and pumps again.
    expect(collectCalls).toBe(1);
    expect(sent).toHaveLength(1);
    handle.close();
  });

  // Spec: a snapshot too large for one SyncUpdate is split into several, each
  // carrying "the maximum timestamp found in the SyncEntity" it delivered. Every
  // Ack therefore makes durable progress, so a backlog that outlives a single
  // connection still drains instead of restarting from scratch each time.
  it('splits an oversized snapshot across updates, each Ack advancing its own checkpoint', async () => {
    const { channel, sent, messages$ } = makeFakeChannel();
    const advanced: number[] = [];

    const handle = startSyncStateMachine({
      peerStatementAccountId: '0xpeer',
      dataChannel: channel,
      collect: async () => ({
        // Two ~33KB entities: over the 60KB budget together, so they must go out
        // as separate updates with their own checkpoints.
        entities: [
          {
            entity: {
              tag: 'ChatsAdded' as const,
              value: Array.from({ length: 1000 }, (_, i) => ({
                tag: 'Contact' as const,
                value: new Uint8Array(32).fill(i % 256),
              })),
            },
            maxTimestamp: 111,
          },
          {
            entity: {
              tag: 'ChatsRemoved' as const,
              value: Array.from({ length: 1000 }, (_, i) => ({
                tag: 'Contact' as const,
                value: new Uint8Array(32).fill(i % 256),
              })),
            },
            maxTimestamp: 222,
          },
        ],
        timePoint: 777,
      }),
      apply: async () => {},
      getOutgoingUpdateTime: async () => 0,
      advanceOutgoingUpdateTime: async (_peer, timePoint) => {
        advanced.push(timePoint);
      },
    });

    await delay(10);
    expect(sent).toHaveLength(1);
    expect(sent[0]!.byteLength).toBeLessThanOrEqual(64 * 1024);

    // Acking the first chunk ships the next AND makes durable progress — this is
    // what stops a large backlog restarting from scratch when the channel drops.
    ack(messages$, decodeUpdateId(sent[0]!));
    await delay(10);
    expect(sent).toHaveLength(2);
    expect(advanced).toEqual([111]);

    // The final chunk's Ack carries the round instant.
    ack(messages$, decodeUpdateId(sent[1]!));
    await delay(10);
    expect(advanced).toEqual([111, 777]);
    handle.close();
  });
});
