# Core Data benchmark baselines

Two baselines: Operation-iOS 2.7.0 (one shared context) and 3.0.0 (writer, observer and readers). One
directory per version, one JSON per scenario and stack variant. Conditions are below; read them before
quoting any number.

## Conditions

Every number on this page was taken on one machine, in one configuration. Both baselines were run the same
way, so the ratios between them are meaningful; the absolute values are not a claim about what a shipping
app does on a phone.

| | |
|---|---|
| Host | Apple M2 Max, 32 GB, macOS 26.2 |
| Toolchain | Xcode 26.6 (17F113) |
| Target | iPhone 16 **simulator**, iOS 18.0 runtime (`F6327B69-0673-48AE-9515-C22A4B8CE8CE`) |
| Build configuration | **Debug** — `SWIFT_OPTIMIZATION_LEVEL = -Onone`, `GCC_OPTIMIZATION_LEVEL = 0` |
| Store | SQLite on disk in a per-test temp directory, persistent history tracking on, `UserDataModel` v49 |
| Scale | `BenchmarkScale.default`, embedded in every JSON |
| Execution | `-parallel-testing-enabled NO`, one simulator, benchmarks excluded from CI and the test plan |
| Sampling | One recorded run of the full set per baseline, each after a warm build on an otherwise idle machine |

Read the numbers with these four caveats:

1. **Debug, not Release.** Mapping a row is pure Swift and runs unoptimized here, so every per-row mapping
   cost is inflated relative to a shipping build. Scenarios dominated by mapping (B3, B4) would improve in
   Release on both sides of the comparison; scenarios dominated by SQLite and the coordinator lock (B1, B2)
   would move much less.
2. **Simulator, not a device.** Storage is the host's SSD through the host filesystem, and the CPU is the
   M2 Max rather than an A-series. Absolute I/O costs and the read/write ratio differ on hardware.
3. **Same machine, two days apart.** 2.7.0 was recorded on 2026-09-18 and 3.0.0 on 2026-09-20, on the same
   host, with nothing else running. Thermal state and background load are not otherwise controlled, and each
   baseline is a single run rather than an average, so treat differences under roughly 10% between columns as
   noise. The effects reported below are 5× to 150×, well clear of that.
4. **Comparable only to each other.** Re-running on another machine produces a different absolute scale, so
   compare a new run against a baseline you took yourself on that machine.

```bash
set -o pipefail && TEST_RUNNER_COREDATA_BENCH_OUT="$PWD/docs/benchmarks/coredata/<date>-operation-ios-<version>" \
  xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appIntegrationTests \
  -destination 'platform=iOS Simulator,id=F6327B69-0673-48AE-9515-C22A4B8CE8CE' \
  -parallel-testing-enabled NO \
  -only-testing:polkadot-appIntegrationTests/CoreDataBenchmarks \
  -only-testing:polkadot-appIntegrationTests/CoreDataTopologySpike 2>&1 | xcbeautify --quiet
```

`-parallel-testing-enabled NO` matters: the scheme allows parallel testing, and with it on xcodebuild runs
every scenario on two simulator clones at once and the timings measure the contention between clones.

## How to read the columns

Each measure is a set of `n` operation latencies (one fetch, one save, one save-to-delivery interval).
`LatencyRecorder` sorts them and reports positions in that sorted list rather than the mean, because
contention shows up as a long tail that a mean hides.

| Column | Meaning |
|---|---|
| `p50` | median: half the operations were faster. The typical, uncontended cost. |
| `p95` | 1 in 20 was slower. Where queue waits and blocked reads appear first. |
| `p99`, `max` | the tail: an operation that queued behind a long transaction. |
| `wall` | first start to last end. What a user feels for the whole phase. |
| `ops/s` | `n / wall`: sustained throughput of all readers or writers combined. |

A p50 far below p95 (2.7.0 B2 read: 12 vs 69 ms) means most operations are cheap and a minority wait behind
something; the gap is the queue wait. Percentile position is `sorted[min(n - 1, floor(n × q))]`, so with
`n = 1` every column shows the same value (B4 registration and wall are single measurements).

## 2.7.0 → 3.0.0

2.7.0 ran `.serial` only, which is all it had. 3.0.0 ran all three variants; the app uses
`concurrent(readerConcurrency: 2)` and the NotificationServiceExtension uses `.serial`. Cells are
p50 / p95 ms unless marked.

| Measure | 2.7.0 serial | 3.0.0 serial | 3.0.0 conc2 | 3.0.0 conc4 |
|---|---|---|---|---|
| B1 read | 11.3 / 11.9 | 11.1 / 11.8 | 7.9 / 8.6 | 5.3 / 5.8 |
| B2 read (writer active) | 12.4 / 68.6 | 11.2 / 57.2 | 8.5 / 9.3 | 7.8 / 8.6 |
| B2 write (100-row batch) | 66.0 / 76.6 | 56.2 / 61.6 | 50.7 / 54.3 | 58.9 / 62.2 |
| B3 save (1 row, 20 subscriptions) | 62.0 / 65.0 | 2.8 / 6.2 | 1.3 / 2.3 | 1.3 / 2.2 |
| B3 save → delivery | 62.0 / 65.0 | 2.8 / 6.2 | 2.3 / 3.7 | 2.4 / 3.5 |
| B3 chatMapperCalls (total) | 225,663 | 3,163 | 3,062 | 3,062 |
| B4 voucher save | 18.0 / 33.2 | 1.5 / 3.1 | 1.0 / 1.5 | 1.0 / 1.2 |
| B4 registration (500 tx, 1 tx) | 586 | 582 | 586 | 585 |
| B4 status save | 1.1 / 2.7 | 1.1 / 2.6 | 1.0 / 1.5 | 1.0 / 1.3 |
| B4 concurrent chat read | 6.7 / 18.4 | 2.3 / 3.7 | 2.2 / 2.5 | 2.2 / 2.4 |
| B4 wall (ms) | 11,164 | 2,678 | 2,306 | 2,283 |
| B4 assetMapperCalls (total) | 376,760 | 4,007 | 2,510 | 2,510 |
| B4 voucher deliveries | 501 | 501 | 489 | 489 |
| S1 nested-child read (during write) | 0.7 / 238.6 | 1.1 / 214.9 | | |
| S1 sibling read (during write) | 0.5 / 1.0 | 0.2 / 0.8 | | |

### Headline, 2.7.0 serial → 3.0.0 concurrent2

| Measure | 2.7.0 | 3.0.0 | Change |
|---|---|---|---|
| B2 read p95, writer active | 68.6 ms | 9.3 ms | 7.4× faster |
| B3 save with 20 subscriptions | 62.0 ms | 1.3 ms | 48× faster |
| B3 save → delivery | 65.0 ms | 2.3 ms | 28× faster |
| B4 voucher save | 18.0 ms | 1.0 ms | 18× faster |
| B4 wall, 500 coins | 11,164 ms | 2,306 ms | 4.8× faster |
| B3 mapper calls | 225,663 | 3,062 | 74× fewer |
| B4 mapper calls | 376,760 | 2,510 | 150× fewer |

### Reading

- **Reads no longer wait for writes.** On 2.7.0 the B2 reader tail (68.6 ms) was exactly one write batch
  (66 ms): a read waited for whatever write was in front of it. On 3.0.0 it is 9.3 ms. B4's chat reader
  shows the same, 18.4 → 2.5 ms p95.
- **Saves no longer pay for subscribers.** On 2.7.0 a one-row insert with the production subscription floor
  cost 62 ms, and the subscriber received its snapshot at the instant the save completed, because every
  subscriber re-mapped its whole result set inside the save. On 3.0.0 the save is 1.3 ms and delivery 2.3 ms.
  Two changes compound here: mapping moved to the observer context, and the snapshot subscriber re-maps only
  the rows the fetched results controller reports as changed.
- **Mapper calls fell by two orders of magnitude**, which is what makes the recycling replica finish in
  2.3 s instead of 11.2 s. B4's remaining 2,510 calls are the initial map plus one per changed row.
- **`.serial` on 3.0.0 is not 2.7.0.** It keeps one context, so reads still queue behind writes (B2 p95
  57 ms), but it gets the cheaper subscriber: B3 save 62 → 2.8 ms. That is the extension's mode, and the
  extension has no subscriptions, so it neither gains nor loses much.
- **Four readers buy little.** B1 improves (7.9 → 5.3 ms p50) because it is nothing but parallel fetches,
  but the B2 writer slows (50.7 → 58.9 ms) from more readers contending for the one coordinator.
  `CoreDataConcurrencyPolicy.app` stays at 2.
- **S1 is why readers are siblings, not children.** A child context's fetch waits for its parent's write
  (p95 215 ms against a 319 ms write); a sibling on the coordinator does not (p95 0.8 ms).

### Where the remaining time goes

Measured per row, our costs are still an order of magnitude above what the engine charges (Core Data fetches
flat rows at roughly 2 µs each), so the remaining headroom is in round trips, not in the store:

- `CoreDataRepository.save` issues one fetch per model before inserting or updating, and the message mapper
  fetches its chat and creates a content row. That is the ~520 µs per row in B2's write.
- Mappers fault relationships row by row; `relationshipKeyPathsForPrefetching` would turn N faults into one
  query per relationship.
- `DurableTxCoreDataRepository.nextSequence` runs a sorted fetch per inserted row, which is most of B4's
  586 ms registration.
