# Durable Transactions Architecture

## Overview

`Packages/DurableTransactions` is the crash-durable "did this transaction land?" engine, shared by every
feature that submits mortal extrinsics whose effects must never be lost or double-spent. It owns the
ledger row, the submission watch, recovery, reorg handling and the block-body search. What a transaction
*means* — which resources it locks and how its effect is observed on chain — belongs to the feature that
registers it, behind one seam: `TxCompletionOracle`.

Coinage (`Packages/Coinage/Sources/CoinageTx/`) is the first domain; installation registration
(`Packages/Coinage/Sources/Installation/Registration/`, domain `coinage-installation`) is the second.

## Key Components

### Engine (`Packages/DurableTransactions`)

| Component | Role |
|-----------|------|
| `DurableTxService` / `DurableTxServicing` | Builds, registers (atomically, with the domain's hook inside the same transaction) and submits; **schedules** rows that have no extrinsic yet; status and group streams; `start()` / `stop()` the head-driven recovery and the builder |
| `DurableSubmissionPolicy` / `DurableSubmissionPolicyRegistry` | The write-side domain seam (below): builds a transaction outside the call that registered it, and again when an attempt is proven unable to land |
| `DurableSubmissionExecutor` | Actor. Watches the ledger's `pendingSubmission` rows, buckets them by `(policyId, groupId)`, one task per bucket, with backoff and a per-row rebuild cooldown |
| `DurableSubmissionLauncher` | Takes ownership of an attempt, writes it onto the row, and hands it to the tracker — the one path both a first submission and a rebuild go through |
| `DurableVerdictWriter` | The single writer of every status write. A `failure` whose policy answers `canRetry` becomes `pendingSubmission` instead |
| `DurableTxAttempt` | One attempt: `txHash`, `checkpoint`, `mortalityBlocks`, derived from a built extrinsic's own `CheckMortality` era |
| `DurableTxRepositoryProtocol` | The ledger row, domain-neutral: id, `domainId`, sequence, group, the current attempt (tx hash, checkpoint, mortality), status, success record, submission policy. Compare-and-set is the only status writer |
| `DurableTxRegistrationScope` | Marker for the store's open write transaction. A domain store writes its rows inside a hook receiving it and must throw `foreignRegistrationScope` for a scope of another store technology |
| `DurableTxTracker` | Follows one built extrinsic from submission; proposes verdicts through the same compare-and-set; releases ownership exactly once |
| `DurableTxOwnershipSet` | Volatile: which *attempt* of which transaction a live submission owns, so a pass skips it. One-shot per attempt, so a rebuild is owned afresh |
| `DurableRecoveryPass` | One pinned view per chain, two rounds per domain (so a domain reasoning over other transactions' statuses sees round-one writes), CAS writes |
| `CompletionLadder` | Rules 0–5 (below) |
| `PinnedChainViewProtocol` / `PinnedChainViewFactory` | The generic chain reads at two pinned heads: block hash at height, block ref by hash, dispatch outcome, body search over a window. Keyed by `chainId` |
| `TxCompletionOracle` / `TxCompletionPassScope` | The domain seam (below) |
| `MonotoneEffectOracle`, `UnobservableOracle` | Ready-made oracles for "an effect that appears and stays" and for "decide by history alone" |
| `DurableChainToolsProviding` | Per-chain extrinsic operation factory and submitter, injected by the app |
| `DurableTransactionsTestSupport` | `FakeChain`, `FakePinnedChainViewFactory`, `StubPinnedChainView`, `InMemoryDurableTxRepository`, `FakeExtrinsicSubmitter`, `StubPassScope` — for any domain's tests |

### App-side (`polkadot-app/Common/DurableTransactions/`)

- `DurableTxCoreDataRepository` over `CDDurableTx`
  opens the one write transaction and hands `CoreDataRegistrationScope` to the domain hook.
- `DurableTxRowObserving` lets a domain react to a status write in the same transaction (coinage
  touches its coin/voucher rows so snapshot subscribers re-emit).
- `DurableChainToolsProvider` resolves extrinsic tools from the chain registry, with the extrinsic
  format decided by `ExtrinsicVersionProvider` on every request (from the current runtime) and the tools
  cached per (chain, format), so a runtime upgrade mid-process switches formats.
  Chains in `signedChains` (Asset Hub, for installation registration) are treated as signed; all others
  as general transactions. One chain cannot host both kinds until the engine models the format per request.
- `ServiceCoordinator.createDurableTransactionEngine` builds the engine once; every domain shares it, and the coordinator alone calls `start()` (after coinage setup) and `stop()` (on throttle). No domain starts or stops the engine.

### Coinage's half (`Packages/Coinage/Sources/CoinageTx/`)

- `CoinageTxService` — the unchanged `CoinageTxServicing` facade; registers asset rows inside the
  engine's hook, maps engine errors to `CoinageTxError`.
- `CoinageAssetLedgerProtocol` — asset rows keyed by the engine's ids, the four registration
  invariants, handoff marks. App implementation: `CoinageAssetLedgerCoreData`.
- `CoinageResourceOracle` — the `TxCompletionOracle`: `CoinageEntryDag` + `CoinageEvidenceCollector` +
  `CoinageRules` answer completion / non-completion per head.
- `CoinageStateReader` — coin, voucher and alias reads at a `BlockRef`.
- `CoinageTxEntry` = engine `DurableTxEntry` + `inputs` / `outputs`. `CoinageTxId`, `CoinageTxStatus`,
  `CoinageTxGroupId` are typealiases of the engine's types; `Coinage` re-exports `DurableTransactions`.

## Status and Attempts

`DurableTxStatus` has five cases. The two predicates over them are deliberately different:

- **`isLive`** — `pending`, `pendingSuccess`, `pendingSubmission`. The transaction holds whatever its
  domain locked for it.
- **`awaitsVerdict`** — `pending`, `pendingSuccess`. Bytes were submitted and nothing has concluded
  about them, so a recovery pass may decide it.

`pendingSubmission` is the gap between them: registered, locks held, nothing on the wire. It is the
executor's, not a pass's — a row with no attempt has no bytes, no window and no inclusion for any rule
to read. `getAllEntries(domain:)` therefore excludes such rows, so neither the pass nor any oracle sees
one.

A row's `txHash` / `checkpoint` / `mortality` describe **the current attempt**, not *the* extrinsic. A
rebuild overwrites them in place and keeps the row's id, group and the domain's rows — which is what
lets a payment made out of a reorged claim still land once the claim is built again. Every status write
is therefore a compare-and-set on `(status, txHash)`: a verdict about bytes already proven unable to
land can never be written onto the rebuilt attempt that replaced them.

A scheduled row still *is* a `DurableTxEntry`, carrying placeholder attempt fields
(`DurableTxSchedule.makeEntry`), so a caller watching its operation group sees the transaction exist and
waits for it rather than reading an empty group as a finished one. Those fields are meaningless until
`withAttempt(_:)` replaces them and nothing reads them while the status is `pendingSubmission`.

## The Seam: `DurableSubmissionPolicy`

The write-side counterpart of `TxCompletionOracle`:

```swift
public protocol DurableSubmissionPolicy: Sendable {
    var chainId: ChainId { get }
    func canRetry(_ entry: DurableTxEntry, params: Data, failure: DurableFailureKind) async -> Bool
    func prepareSubmission(_ transactions: [ScheduledDurableTx])
        async throws -> [DurableTxId: SubmissionPreparation]   // .ready(ExtrinsicBuiltModel) | .giveUp
}
```

1. **`canRetry` must not read the chain.** It is asked while a verdict is being written. Whether a
   rebuild is still *possible* belongs to `prepareSubmission`, which may suspend for as long as it needs.
2. **A failure kind bounds the loop.** `DurableFailureKind` is `.expired`, `.dispatchFailed` or
   `.rejected`. An attempt that was simply never included may be rebuilt however late; one that was
   dispatched and failed, or refused outright, would most likely fail the same way — nothing else stops
   a repeating failure from being rebuilt for ever.
3. **A thrown error is never a verdict.** `prepareSubmission` throwing just means the call is made again
   after a backoff; only `.giveUp` fails a transaction.
4. **`params` are opaque to the engine** and stored as given, so a policy owns their encoding *and its
   evolution* — a shape change needs a versioned decoder for the rows already written.

Policies are registered into `DurableTxService.policies` before anything is scheduled. A row naming an
unregistered policy is abandoned by the executor rather than left waiting for ever, and a verdict for it
is written as the failure it already was.

## The Seam: `TxCompletionOracle`

```swift
public protocol TxCompletionOracle: Sendable {
    var chainId: ChainId { get }
    func openPass(transactions: [DurableTxEntry], ledger: any LedgerView, view: any PinnedChainViewProtocol)
        async throws -> any TxCompletionPassScope
}

public protocol TxCompletionPassScope: Sendable {
    func provenCompleted(_ tx: DurableTxEntry, at head: HeadKind) -> Bool
    func provenNotCompleted(_ tx: DurableTxEntry, at head: HeadKind) -> Bool   // default false
}
```

Three things about the shape are deliberate:

1. **Both questions are positive, and neither firing means "unknown".** An unknown transaction falls
   through to the block-body search, which is ground truth. The engine never negates a read, so a
   transport error cannot become a verdict.
2. **`provenNotCompleted` defaults to false.** At the finalized head, past mortality, it becomes a
   terminal failure that releases whatever the domain locked. Leaving it false costs one block scan;
   getting it wrong costs a double spend. A domain opts in only when its observation is monotone and
   singly written.
3. **The scope does not suspend.** Every chain read happens in `openPass`, batched; a per-transaction
   read during rule evaluation is not expressible.

`LedgerView` hands the oracle every transaction of its domain with the status the pass snapshotted, so a
domain whose answers depend on other transactions (coinage: a finalized successor proves the minter ran)
reads them there. Dependencies are therefore *implicit* — the engine stores no edges.

## The Ladder

Evaluated in order per transaction; first match wins. `F` is the finalized head, `B` the best head.

| # | Condition | Verdict |
|---|-----------|---------|
| 0 | a success block is recorded → is it still canonical? (read once per height per pass; unreadable → undecided) | canonical ≤ F → `finalizedSuccess`; canonical > F → `pendingSuccess`; gone → re-ask oracle at F, then B, else demote to `pending` and clear |
| 1 | `provenCompleted(F)` | `finalizedSuccess` |
| 2 | `provenCompleted(B)` | `pendingSuccess` recorded at B |
| 3 | window closed ∧ `provenNotCompleted(F)` | `failure` |
| 4 | window open ∧ `provenNotCompleted(B)` | `pending` (no body search this head) |
| 5 | body search over `[checkpoint … min(mortalityEnd, F)]` | found + success → `finalizedSuccess`; found + failed → `failure`; unreadable → `pending`; absent, whole window read, window closed → `failure`; else `pending` |

The mortality window is the extrinsic's own `CheckMortality` era, read off the built model at
registration — the window the runtime enforces, so the search covers exactly the blocks the extrinsic
could have landed in.

## Registration Atomicity

Registration commits before submission, so no extrinsic is ever in flight without a record holding what it
locks. The engine store opens one transaction, inserts its rows, then runs the domain hook with the
scope; the domain validates and writes its rows in that same context. A throw from the hook rolls both
back. Ownership is taken inside the same transaction, so a pass can never reach a committed row before its
watcher. A domain store never opens a transaction of its own inside the hook (the shared serial CoreData
queue would deadlock).

## Adding a Domain

1. Pick a `TxDomainId` and the `chainId` its transactions live on.
2. Implement `TxCompletionOracle` (or use `MonotoneEffectOracle` / `UnobservableOracle`) and register it
   with `engine.oracles` before the first submission.
3. Keep whatever the transaction locks in the domain's own rows keyed by `DurableTxId`, written inside
   the registration hook through the scope the engine hands you.
4. Submit with `engine.submitTransactions(domain:requests:groupId:onRegister:)`; observe with
   `subscribeTransactionStatus` / group streams.
5. Test over `DurableTransactionsTestSupport` fakes; the engine's own suite
   (`Packages/DurableTransactions/Tests`) needs no domain types.

Worked example — `coinage-installation`: the group id is `"{contractHex}/{installationHex}"`, so the
oracle (`CoinageInstallationRegistrationOracle`, a `MonotoneEffectOracle` on Asset Hub) reads each
contract's list once per head and answers `true`/`false` per registration; a failed read answers nothing.
Nothing is locked, so there is no registration hook. The registrar never submits while a group entry is
live — the oracle credits the same record to every attempt — and retries with backoff once the last
attempt has settled without a `finalizedSuccess`.

## Hard Rules

1. **Terminal verdicts rest on finalized facts.** Only the finalized-bounded search, Rule 3 at F, and a
   pre-submission validation refusal may write `failure`; only F-level evidence writes `finalizedSuccess`.
1b. **Every status write goes through `DurableVerdictWriter`.** It is the one place that offers a
   failure back to the transaction's policy before it becomes terminal. A writer that bypasses it is a
   path that can forget to retry. A policy that cannot be *read* fails the write rather than the
   transaction: the verdict is re-derived next pass, while a failure written now could never be taken back.
2. **Unknown is never a verdict.** A failed read leaves a transaction undecided; every predicate is
   positive-form.
3. **Rows are never deleted.** Terminal rows are history a domain's provenance reads still need.
4. **One writer at a time.** A submission-owned transaction is skipped by the pass; every write is a
   compare-and-set against the status the verdict was formed from.
5. **Package tests run only through the test plan.** `DurableTransactionsTests` is listed in
   `polkadot-app/polkadot-app.xctestplan`; run with `-scheme polkadot-app -only-testing:DurableTransactionsTests`.

## Seams

| Seam | Where | When to touch |
|------|-------|---------------|
| Engine API | `Packages/DurableTransactions/Sources/DurableTransactions/Engine/DurableTxService.swift` | New engine capability every domain needs |
| Submission seam | `.../Engine/DurableSubmissionPolicy.swift` | Changing what a domain can build or rebuild |
| Builder pacing | `.../Engine/DurableSubmissionExecutor.swift` (`Timing`) | Backoff and rebuild cooldown |
| Verdict routing | `.../Engine/DurableVerdictWriter.swift` | How a failure is offered back to a policy |
| Ladder | `.../Engine/CompletionLadder.swift` | Only for a rule true of *any* transaction |
| Domain seam | `.../Oracle/TxCompletionOracle.swift` | Changing what a domain can tell the engine |
| Chain reads | `.../Chain/PinnedChainView*.swift` | New generic chain access |
| App store | `polkadot-app/Common/DurableTransactions/` | CoreData shape of the ledger row, chain tools |
| Coinage oracle | `Packages/Coinage/Sources/CoinageTx/Engine/CoinageResourceOracle.swift`, `CoinageRules.swift` | Coin / voucher evidence semantics |
| Coinage ledger | `Packages/Coinage/Sources/CoinageTx/Store/CoinageAssetLedgerProtocol.swift`, `polkadot-app/Modules/Coinage/Model/CoreData/CoinageAssetLedgerCoreData.swift` | Asset rows, invariants, handoffs |
