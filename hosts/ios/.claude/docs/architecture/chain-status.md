# Chain Status: Liveness Indication

## Overview

The app monitors connection state and block production across three chains (Individuality, Bulletin, Asset Hub), computing liveness as the fraction of block slots in a trailing window that produced a block. This feeds into a pure indication function that renders one of three states — normal, dead, or outage — as a ring on the status strip. The signal propagates from network status through debounce, anchor fetch, sample windows, indication derivation, dwell, and emission with no flicker.

## Pipeline

```
NetworkStatus (per chain)
  ↓ (300ms debounce, connect events only)
ChainStatusProvider.statuses
  ↓ (on connect transition, on foreground)
ChainLivenessAnchorProvider.fetchAnchor (4 RPC reads)
  ↓
ChainLiveness.apply (seeded history)
  ↓ (on block arrival)
ChainStatusProvider.blocks → ChainLiveness.record
  ↓
ChainLiveness.liveness(at:) → ChainStatusIndication.resolve
  ↓ (dwell guard on enter-dead)
ChainStatusProvider.previousIndications
  ↓ (1-second emission tick, gated on row set change)
rowsSubject.send
```

**Key ownership:** `ChainStatusProvider` is an actor that owns all statuses, blocks, per-chain `ChainLiveness` values, the re-anchor hold, dwell bookkeeping, and wiring between subsystems. `ChainLiveness` is a pure struct holding all window arithmetic; it never reads the current time — every entry point takes a `Date`. `ChainLivenessAnchorProvider` is an actor performing four RPC reads per anchor. `ChainStatusIndication.resolve` is pure, total, and clock-free.

## Per-Chain Constants

Individuality (`.chat`, 2-second period) and Asset Hub (`.assethub`, 2-second period) use identical block periods. Bulletin (`.bulletin`, 6-second period) is slower.

Window is `max(30s, 10 × period)`:
- Individuality, Asset Hub: `max(30s, 20s) = 30s` window, `slotCount = 15` blocks
- Bulletin: `max(30s, 60s) = 60s` window, `slotCount = 10` blocks

Bulletin's 60-second window is intentional. A flat 30-second window would give Bulletin `slotCount = 5`, where the `5/6` outage threshold demands a perfect run of blocks — a single late block reads as a 288-degree red arc. The 10-block window preserves the `5/6` ratio while raising the sampling resolution so minor lags do not trigger alarms.

## Liveness: Definition and Computation

Liveness is `min(1.0, heightDelta / slotCount)` where `heightDelta` is the block count from the sample at or before the window start to the newest sample. It is `nil` until a full window has elapsed since the first recorded sample. `ChainStatusIndication.resolve` maps `nil` to `.normal` — optimistic, not alarming, on a cold start.

This is an **average rate over the last Nmax blocks, not an exact count** in the trailing window. A burst followed by freeze looks identical to uniformly slow production; an exact count would require searching block hashes by timestamp, rejected on RPC cost.

## Two Distinct Underflow Guards

Do not conflate these:

1. **Reorg clamp in `ChainLiveness.liveness(at:)`:** `BlockNumber` is unsigned. A reorg deeper than the window would put the head below the anchor and the subtraction would trap before any clamp ran. The code compares first: `let heightDelta = newestSample.height >= anchor.height ? newestSample.height - anchor.height : 0`.

2. **Chain-too-short check in `ChainLivenessAnchorProvider.fetchAnchor`:** Before reading block hashes at `height - slotCount`, the provider verifies `height >= slotCount`. A chain shorter than the window throws `.chainTooShort` before any subtraction.

## Window Eviction

`dropExpiredSamples` retains the newest sample at or before the window start rather than dropping it. That sample is the anchor from which the block count is measured; dropping it would let a stalled chain read as live.

## Anchoring

An anchor is a `(block height, on-chain timestamp)` pair that places locally-observed block arrivals on the chain's clock. It runs exactly four RPC reads:

1. Resolve the head block height once (`blockInfoProvider.fetchCurrent`)
2. Derive both block hashes from that same height:
   - Head hash at `height`
   - Previous hash at `height - slotCount`
   
   (Taking best hash and best height as separate calls would leave them a block apart, flattening liveness.)

3. Read `Timestamp.Now` at the head hash
4. Read `Timestamp.Now` at the previous hash

The span is `timeHead - timePrev`. A non-positive span throws `.invalidTimeSpan`.

### Apply: Seeding History from an Anchor

In `ChainLiveness.apply`, a non-positive span returns without anchoring — defence in depth, deliberately duplicating the provider's check, because mapping it to a full window would report a healthy chain on input already known to be inconsistent.

The effective slot count is:
- `span <= windowSeconds`: `effectiveSlots = slotCount` (the entire window is covered)
- `span > windowSeconds`: `effectiveSlots = floor(slotCount × windowSeconds / span)` clamped to `0...slotCount` (the on-chain time covers more than our window; compute the fraction)

The method clears history and seeds two synthetic samples:
- `height = headHeight - effectiveSlots` at `date - windowSeconds`
- `height = headHeight` at `date`

The ordinary window arithmetic then yields `effectiveSlots / slotCount` with no second code path and the full-window guard passes immediately.

When `headHeight < effectiveSlots` (the chain is shorter than the window), the fallback seeds:
- `height = 0` at `date - windowSeconds`
- `height = headHeight` at `date`

No exception is thrown; this allows young chains to report liveness immediately.

## Anchor Timing and Cancellation

Anchors run on connect (when the socket transitions to `.connected`) and on foreground (when the app returns from background), never continuously. Between anchors, block arrivals are tracked against the device clock.

`anchorGeneration[target]` makes superseded completions no-ops. A pending anchor fetch is superseded by a new connect or foreground event; incrementing the generation and checking it on completion ensures the old result is discarded.

**This must NOT be replaced by a check on `awaitingReanchor` membership.** The connect path never inserts into that set before starting the anchor, so gating on membership would disable anchoring on connect entirely.

The probe is bounded by a 15-second `withTimeout` (`anchorTimeout`). Without it a hung RPC would leave the target in `awaitingReanchor` forever, freezing its row on the last indication.

## Foreground Re-Anchor Hold

`handleForeground` inserts every connected target into `awaitingReanchor` and starts an anchor. While held, `indicateRows` returns the previous indication and previous liveness instead of the freshly computed pair.

**Exception:** if the computed value is `.dead`, it is never held. Dead is state-driven (socket offline) and must not sit behind a stale-liveness hold. A failed probe releases the hold and clears history, so the row reads normal rather than revealing a stale-derived outage.

## Reconnect: Clearing Prior History

Any status other than `.connected` clears the block info, clears liveness history, and calls `blockProvider.clear(for:)`. Pre-drop arrivals never contribute to liveness measured on a new connection.

## Dead Dwell

A 3-second minimum is applied only to transitions *into* dead, the one state driven by a bursty socket-level signal. Leaving dead is immediate. A row that has never been emitted skips the hold entirely, so a cold launch with no connectivity reads dead at once instead of normal for three seconds.

## Rendering

The status ring is drawn in three ways:

- **Normal:** Filled disc, `.fgPrimary` fill with the chain icon knocked out in `.bgSurfaceMain`.
- **Dead:** Icon and surround in `.fgTertiary` (muted grey). While connecting, the icon pulses; once the socket stops retrying, it sits still.
- **Outage:** Unfilled ring with a partial arc. The arc length is `liveness × 360°`, counterclockwise from 12 o'clock. Colour depends on severity.

### Colour Banding (Under Review)

`ChainStatusRingStyle.arcBand(forLiveness:)` maps liveness to one of four bands:

| Band | Range | Colour |
|------|-------|--------|
| critical | < 0.25 | red (error) |
| degraded | < 0.5 | amber (warning) |
| fair | < 0.75 | green (success) |
| nearNormal | ≥ 0.75 | `.fgPrimary` (monochrome) |

## Connection Status Panel

The connection panel displays average block interval as `expectedBlockSeconds / liveness`, undefined when liveness is zero or nil. The code guards `liveness > 0` before the division.
