import { Subject } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ICE_BATCH_WINDOW_MS, bufferIceCandidates } from './iceBatching';

// The operator only moves candidates through a buffer — it never reads a field —
// so an opaque stand-in is enough.
// eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- opaque stand-in; the operator reads no fields
const candidate = () => ({}) as RTCIceCandidate;

describe('bufferIceCandidates', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('emits as soon as the batch is full, without waiting out the window', () => {
    const source = new Subject<RTCIceCandidate>();
    const batches: RTCIceCandidate[][] = [];
    source.pipe(bufferIceCandidates()).subscribe(batch => batches.push(batch));

    for (let i = 0; i < 4; i++) source.next(candidate());

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(4);
  });

  it('emits a partial batch at the window boundary', () => {
    const source = new Subject<RTCIceCandidate>();
    const batches: RTCIceCandidate[][] = [];
    source.pipe(bufferIceCandidates()).subscribe(batch => batches.push(batch));

    source.next(candidate());
    source.next(candidate());
    expect(batches).toHaveLength(0);

    vi.advanceTimersByTime(ICE_BATCH_WINDOW_MS);

    expect(batches).toHaveLength(1);
    expect(batches[0]).toHaveLength(2);
  });

  // The empty batch is load-bearing: it releases an offer/answer whose peer
  // gathered nothing, instead of hanging it forever.
  it('emits an empty batch at the window boundary when nothing gathered', () => {
    const source = new Subject<RTCIceCandidate>();
    const batches: RTCIceCandidate[][] = [];
    source.pipe(bufferIceCandidates()).subscribe(batch => batches.push(batch));

    vi.advanceTimersByTime(ICE_BATCH_WINDOW_MS);

    expect(batches).toEqual([[]]);
  });
});
