# Coinage Architecture

## Overview

Coinage is the payment primitive for the Polkadot app — managing digital coins with derivation indices, values, and lifecycle states. Used for in-app payments and peer transfers.

## Key Components

### Package
- **Coinage** (`Packages/Coinage/`) — core coinage logic, coin models, transfer planning

### Module
- **Coinage** (`Modules/Coinage/`) — UI for coinage operations

### Services (in ServiceCoordinator)
- `coinageService` — coinage state management
- `coinageBackupSyncService` — the wallet's "balance restored" card: visibility from the installation rows, spinner from `BackupProgress`

## Coin Model

A coin (`Packages/Coinage/Sources/Models/Coin.swift`) has:
- `exponent` — coin value as a power of two (`2^n`)
- `derivationIndex` — a `CoinageKeyIndex`: the installation it was allocated in and its item there
- `age` — on-chain age; `nil` = never seen on chain, `0` = fresh from unload/split
- `isOnchain` — on-chain presence, written only by chain sync (`age != nil ∧ ¬isOnchain` = seen then vanished)
- `handoffMark` — whether the coin has been handed off to a peer, and how far along
- `publicKey` — on-chain account id derived from `derivationIndex`, cached so the durability layer never re-derives it

## Installations (per-installation derivation page)

Every installation draws a random 32-byte `CoinageInstallationId` (`Packages/Coinage/Sources/Installation/`)
and allocates coins and vouchers under it: `//coinage//4294967295//0x{id}/{item}` for coins,
`//coinage-ring-vrf//4294967295//0x{id}//{item}` for vouchers. A `CoinageKeyIndex(installation, item)`
addresses one key; its string form `"{idHex}/{item}"` (`toString()` / `fromString(_:)`) is the CoreData
`identifier`, written but never parsed back — rows carry the split `installationId` / `derivationIndex` columns
(`CDCoin.keyIndex()`), and the location pipeline keys member subscriptions by voucher public key. `item` is a
`DerivationIndex` (`UInt64`); CoreData holds it as `Int64(bitPattern:)` and reads it back with
`UInt64(bitPattern:)`, the same for the scan cursors.

The *current* installation is one database row (`CDCurrentInstallation`, behind
`CoinageCurrentInstallationRepositoryProtocol`) in the same store as the coins and vouchers allocated in it,
read once per process by `CoinageCurrentInstallationStore` (`CoinageCurrentInstallationStoring`) since it
never changes once created; the read and the insert share one context block so concurrent first callers
get one id. Its two allocation counters live in the Keychain, scoped by the installation itself:
`io.polkadotapp:<installationIdHex>:coinage.coin.index` and `…:coinage.voucher.index` (SCALE `UInt64`, the
next unissued item; `CoinageInstallationKeychainTags` supplies the layout). Each `next…Item` call reserves
an item for good, so a crash before the coin is saved costs one unused key, never a reused one; previous
installations' rows never move the counters. Putting the identity in the database is what makes a
same-device restore safe: the Keychain (`ThisDeviceOnly`) comes back but the backup-excluded store does
not, so a new installation starts, its counters begin at zero under its own tags, and the old one is
recovered through the contract like any previous installation instead of keeping its counters while its
items are never scanned. The database (`CDCoinageInstallation`) holds *previous* installations only, with
their scan cursors. The `coinageSyncNeeded` / scan-horizon settings are gone. Android keeps the current
installation in the same Room table under an `isCurrent` flag; iOS keeps a separate entity and the
counters in the Keychain.

Backup: the seed's `//datastore` sr25519 account (public key, 64-byte secret and encryption key pinned
against polkadot-js and Android in `DataStoreKeyParityTests`; the encryption key is keyed by the secret in
schnorrkel's ed25519 form via `DataStoreAccountKeys.ed25519Form(of:)` over NovaCrypto's `SNPrivateKey.toEd25519Data()`,
since the iOS SDK holds the canonical scalar) registers each installation in the `AccountDataStore`
contract on Asset Hub (record = ChaCha20-Poly1305 of the id under `"encryption".blake2b32WithKey(secret64)`,
deterministic nonce). Contract reads and the `Revive.call` extrinsic go through the `Revive` package
(`ReviveContractApiProtocol`, `RevivePallet.Call`; see architecture/revive.md); the contract ABI table is
`AccountDataStoreAbi` over `EvmAbi`, and the address is an `EvmAddress`. Registration is a durable-transaction domain (`coinage-installation`, see
architecture/durable-transactions.md); `CoinageInstallationRegistrar` runs it once per process and
reports `CoinageAccountBackupStatus` (`registering / delayed / completed`) — Asset Details shows a warning
row while `delayed`. Recovery (`CoinageBackupRecoveryService`) lists the contract on every launch (three attempts through
`withRetry`; a listing that still fails with no previous installation known ends the launch pass as `.failed`
rather than "nothing to recover", while a missing contract address only skips discovery), records
the other installations as previous ones (`CDCoinageInstallation`) and gap-scans them (batches of 500,
stop after 4 empty in a row, cursors persisted); "Update" deep-searches 10 more batches, "Close" confirms
every installation known (`CDCoinageInstallation.isUserConfirmedCompletion`, `markAllUserConfirmed()`). The
wallet card is shown while any previous installation has `initialScanCompleted && !isUserConfirmedCompletion`
(`CoinageBackupSyncService` watches the rows through `subscribeSnapshot`), so a running or failed deep search
never hides an offered balance; progress only drives the card's "Update" spinner. An installation recorded
after a confirmation is unconfirmed by default, so it is shown again. A scan that could not read the chain ends the phase as `.failed`: no balance is
offered for acceptance, the app's recovery state is `failed`, and the next launch scans again. Recovered
coins keep their absolute index; rows the store already holds are never overwritten. The contract address comes from remote config `account_data_store_config`.

## Key Rules

1. **Use the `KeyDerivation` package** — never hand-roll coin keypair derivation. The derivation domain must be coinage-specific (never reused across features).
2. **Zero balance handling** — properly handle zero-balance edge cases
3. **Deterministic fund amounts** — top-up affordances (the "+" fund button) must compute the resulting coin set from a deterministic plan: same input balance + same target → same coin denominations every time. Don't sample randomly, don't depend on iteration order over an unsorted set, and don't let the displayed preview drift from the amount actually submitted. The fix was switching from "pick coins as we go" to producing a stable plan up-front, then rendering and submitting from that plan.
4. **Exact-match edge in transfer planning** — when the requested amount exactly equals one of the candidate coin denominations, the planner must take the single-coin path and skip the split/unload branches. Hitting split logic with an exact match crashes because it tries to break a coin it doesn't need to. The same care applies to the "no split needed" boundary in `Coinage` transfer planning: always test the exact equality case alongside under/over.

## Balance (strategy-aware, two-pass)

`CoinageBalanceService` emits a single `CoinageBalance` with three plank buckets:

- `availablePrivate` — spendable now at no privacy cost (coins the strategy leaves usable + usable vouchers).
- `gainingPrivacy` — `{ amount, canSpendWithConfirmation }`; deliberately held back, some strategies release it behind a confirmation (`maxPrivacy` does not).
- `pending` — arriving, or past the chain's age ceiling; never spendable.

Derived: `available` = `availablePrivate (+ gainingPrivacy.amount when canSpendWithConfirmation)`; `total` = all three. Amounts are planks — render via `CoinageBalanceServiceProtocol.denominationContext` (`decimal(fromPlanks:)`).

**Consumer semantics**: spend-sufficiency and the primary "spendable" figure use `availablePrivate`; the reachable-with-confirmation figure uses `available`. Products sufficiency → `availablePrivate`; TransferAmount `secured`/`lowPrivacy` → `availablePrivate`/`gainingPrivacy.amount` (the latter zeroed when `!canSpendWithConfirmation`); AssetDetails total → `total`, locked → `total − available`.

**`BalanceEvaluationMode` (immediate | complete).** The balance renders nothing until the first verdicts land, so `CoinRecyclingEvaluator` runs a two-pass: when it has no verdicts yet it first publishes an `immediate` pass (no chain read — `RingCapacityProviding.peekCapacities` returns only memoised capacities, and `RecyclingStrategyProviding.coinStrategy(for:mode:)` skips the quota read, treating quota as exhausted so only the chain age-ceiling gates), then a `complete` pass consults every limit and corrects. The correction is downward-only — a coin the policy would hold shows spendable until `complete` moves it, never the reverse. Recycling is triggered only off the `complete` pass. The balance service itself uses `immediate` (peek) capacities so it never blocks.

## Transfer Planning

Transfer plans determine how coins are spent:
- Exact coins — use specific coins
- Split — divide coins for partial amounts
- Unload and split — complex multi-step transfers

## Payment Processing

- `CoinagePaymentProcessingExtension` watches on-chain events for payment confirmations
- Integrates with chat for payment request/confirmation messages
- See `architecture/chat-extension.md` for chat integration

## Submission Policies (retries)

A coinage transaction proven unable to land is **built again** rather than failed. Each of the three
retriable kinds registers a `DurableSubmissionPolicy` (see architecture/durable-transactions.md) at
`CoinageService.make`, keyed by id: `coinage-split`, `coinage-unload`, `coinage-claim`.

All three are one generic — `InputGatedSubmissionPolicy` — composed with a `CoinageRebuild` that only
knows how to read one kind of transaction back from the ledger, name its inputs, watch them and build
it. Deciding *when* to wait, build, give up or retry belongs to the policy alone, so that behaviour is
written and tested once.

- **Gate**: a rebuild waits until every input it spends is present on chain. `awaitInputs` holds out
  30s once *some* inputs are visible (an input still landing is the ordinary reason a look is
  incomplete), gives up a call after 5 minutes of nothing visible, and wakes at the deadline rather
  than waiting on the chain. The newest look wins outright even when narrower — a fork can take an
  input away, and building against the widest view ever seen would spend what is no longer there.
- **Presence**: coins come from the chain (`CoinOnChainQuerying.subscribeCoinInfos`, a storage
  subscription whose accumulator drops a key that goes absent). Vouchers come from **our own rows**
  (`DatabaseDependencyFactoring.makeTrackedVoucherSnapshotStream()`, filtered to `recycler != nil`) —
  deliberately not a chain read, because those are the same rows the rebuild builds from, so the gate
  and the build can never disagree. A chain read could say "in a recycler" while the row the call is
  built from still has none, and the build would then fail on every attempt until location sync caught
  up. A voucher counts as present while it sits in a recycler: that is where an unload proves it.
  Either way a read that fails is never emitted as a look — it must not erase what the chain last
  showed; a coin subscription that drops instead ends the wait, and the executor's backoff opens a
  fresh one.
- **Params** (`CoinageSubmissionParams`) are SCALE and persisted with the row, so a shape change needs
  a versioned decoder. A transfer carries `buildUntil` + `retryFailures`; a claim carries `retryUntil`
  and the peer's key, which only the payment message holds. The transfer window is
  `CoinageConstants.claimRetryWindow` (6h) — the same one the recipient's claim gets, so neither side
  gives up while the other still tries.
- **Bounding**: `retryableFailure` lets an `.expired` attempt be rebuilt however late, but a
  `.dispatchFailed` or `.rejected` one only while the window is open — nothing else stops a failure
  that always repeats from being rebuilt for ever.
- **Building** is shared with the first attempt through the extracted declarers
  (`SplitExtrinsicBuilder`, `UnloadExtrinsicBuilder`, `ClaimExtrinsicBuilder` over
  `CoinageExtrinsicParts`), which go through `DurableTxServicing.buildExtrinsics` — one build path, not
  two. An unload resolves a **fresh** free token and recycler revision on every build and notes the
  quota only once extrinsics actually exist; it re-reads its vouchers after the look so each carries
  the recycler location it is proven in now.
- **Claims register once.** `ClaimCoinsService` stops when every coin *has* a claim
  (`coins − settled.receivedPublicKeys()`), not when every claim finalized. Rebuilding a failed claim
  is the policy's job, into the coin that claim recorded — a claim retried into a fresh coin would
  strand a payment already registered against the first one.
- **A window closes only after a look.** `ClaimCoinsService` and `ClaimAssetService` end on
  `retryUntil` only once the run has received a look from the chain. A pass past the window with no
  look (offline, dropped subscription) keeps waiting, and the coin subscription is reopened after
  `resubscribeDelay`, so every claim gets at least one attempt with a live connection.

Recycling, voucher loading, offramp unloads and installation registration register with no policy and
keep failing terminally.

## External Payments (offramp)

`Packages/Coinage/Sources/ExternalPayment/` moves coins to a destination account for a product
 or the in-app pay flow (`ExternalPayment.nativeProductId`). Rows are
`ExternalPayment` (`CDExternalPayment`) keyed `external-payment:<productId>:<paymentId>`.

- **Planner** (`ExternalPaymentPlanner`): private vouchers alone, else every on-chain voucher
  (private first, then largest) → `.unloadVouchers`; else coins for the shortfall with all vouchers
  offboarded as they are → `.loadCoins(coins, exactVouchers)`; else `.notEnoughBalance`.
   "Private" is what the balance calls usable (`ExternalPaymentAssetClassifier`).
  `canPayPrivately` is the first step alone, so the warning and the plan cannot disagree. Callers
  warn when it is false and the preset is not `minPrivacy`.
- **Worker** (persist-per-transition, one run per payment, no retries): `Plan` → `OffboardVouchers`,
  or `Plan` → `OnboardCoins` → `OffboardVouchers`. Onboarding recycles under `<id>:recycle`, awaits
  `CoinageRecyclingServicing.observeRecycling` (`pending | allRecycled(vouchers:finalized:) | incomplete`;
  best-block inclusion is enough), then `pickOffboarding` selects what to unload from exact + recycled
  vouchers (largest first; a shortfall fails). The picked vouchers and their surplus are planner output
  persisted with the offboarding stage (`plannedVoucherIndices`, `surplusInPlanks`) and handed to the
  unload as they are. Offboarding submits under `<id>`: success completes, `partialSuccess` is terminal
  (`partiallyCompleted` with `settledInPlanks`), everything else fails. Each stage persists its
  vouchers (`plannedVoucherIndices`) and re-joins its durability group on relaunch; an onboarding row
  whose recycling group cannot be found fails rather than re-plan (nothing may be spent twice).
- **Status**: unknown id → the stream throws `notFound`; duplicates collapse; ends after the first
  terminal status. 
- **Tests**: `Packages/Coinage/Tests/ExternalPayment/`, sweep
  `Packages/Coinage/Tools/external_payment_mutation_sweep.py`.

## Seams

| Seam                    | Where                          | When to touch                    |
|-------------------------|--------------------------------|----------------------------------|
| Coin models             | `Packages/Coinage/`           | Coin structure changes           |
| Transfer planning       | `Packages/Coinage/`           | New transfer strategies          |
| External payments       | `Packages/Coinage/Sources/ExternalPayment/` | Offramp identity, retry, status, planner scope |
| Coinage UI              | `Modules/Coinage/`            | Coinage screen changes           |
| Backup sync             | ServiceCoordinator             | Backup/restore flow changes      |
| Installation identity   | `Packages/Coinage/Sources/Installation/`, `CoinageCurrentInstallationStore` (current row + Keychain counters), `CoinageCurrentInstallationCoreDataRepository`, `CoinageInstallationCoreDataRepository` (previous) | Key index / page format, allocator counters |
| Installation Keychain tags | `Common/Crypto/KeystoreTag.swift`, `Modules/Coinage/Model/Keychain/CoinageInstallationKeychainTags.swift` | Tag layout, installation scoping |
| Installation registration | `.../Installation/Registration/`, `ServiceCoordinator.createInstallationDependency` | Contract ABI, PGAS provisioning, registrar timing |
| Contract calls          | `Packages/Revive/` (see architecture/revive.md) | Runtime API encoding, revert handling, `EvmAddress` |
| Backup recovery         | `Packages/Coinage/Sources/Backup/`, `CoinageBackupSyncService` | Scan rules, progress model, restored-balance card |
| Durability (oracle, asset ledger) | `Packages/Coinage/Sources/CoinageTx/` | Coin/voucher evidence or invariants; the engine itself is `Packages/DurableTransactions` (see architecture/durable-transactions.md) |
| Retry policies | `Packages/Coinage/Sources/CoinageTx/Submission/` | When a transaction is rebuilt, what it waits for, how long |
| Extrinsic declaration | `Packages/Coinage/Sources/Transfer/Plan/Builders/` | The call and origin of a split, unload or claim — shared by the first attempt and every rebuild |
| Instance ID config      | `AppConfig.Coinage.instanceId` | Remote config schema or app instance strategy changes |
