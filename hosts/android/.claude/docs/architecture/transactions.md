# Transactions & Custom Origins

> **What this doc is:** rules for the extrinsic pipeline and custom origins.
> **What this doc is NOT:** a substrate primer. Read `feature/transactions/api/**` for that.

Classic signed extrinsics + a growing family of custom origins (People-Lite, AsPersonalAlias*, AsCoin, AsPgas, etc.). New origins plug in via composition; the core never grows special cases.

---

## Glossary

- **`TransactionOrigin`** = `(signerSource, paysFees, applyTo)`. The interface every origin implements.
- **`TransactionSignerSource`** — sealed: `None` | `Signed { FromAccount(metaAccount) | FromKeyPair(keypair, encryption) }`.
- **`SignedTransactionOrigin`** — classic signed tx; `paysFees = true`; applies `RestrictOrigins(false)`.
- **`SetTransactionExtensionOrigin(signerSource, extension)`** — wraps a custom `TransactionExtension`; typically `paysFees = false`. **This is the composition point for new origins.**
- **`TransactionExtension`** — substrate-sdk-android signed-extension interface. Custom subclasses (`AsCoin`, `AsPgas`, `AsPersonalAliasWithProof`, etc.) override `explicit(...)`.
- **`ExtrinsicService.submitExtrinsic` / `submitAndWatchExtrinsic`** — submission entry point. Use the lambda + origin model; never hand-build extrinsics outside this seam.
- **`ExtrinsicBuilderSequence`** — auto-nonce-incrementing iterator for multi-extrinsic batches keyed by `(ChainId, AccountId)`.
- **`ChainConnectionRefCounter`** — ref-counted connection enabler for background work. `withConnectionEnabled { ... }` for scoped use; `requestConnectionEnabled` + `release()` for long-lived (Service).
- **`DurableTransactionService`** — the durable ledger: `submit` / `submitAll` register signed extrinsics, `schedule` registers transactions that are built later. Statuses `PENDING` → `PENDING_SUCCESS` → `FINALIZED_SUCCESS` | `FAILURE`, plus `PENDING_SUBMISSION` (waiting for its policy to build it; live, holds its domain's locks, never evaluated by a pass).
- **`AsyncDurableSubmissionPolicy`** — builds a durable transaction's extrinsic outside the registering call: the first build of a scheduled row, and a rebuild of a row whose attempt is proven unable to land. Bound per policy id (`@IntoMap @SubmissionPolicyKey`), referenced per row with opaque params.

---

## Rules

1. **`blocking`** — Background work submitting extrinsics uses `ChainConnectionRefCounter.withConnectionEnabled(...)`. The default chain connection isn't active off-screen.
2. **`blocking`** — Keypairs are never shared across roles (identity ≠ device key ≠ wallet). Each role derives from its own path.
3. **`blocking`** — Multi-extrinsic batches where proof count must match on-chain operation count are not truncated to fit a cap. Plan the batching properly.
4. **`major`** — A new origin family adds to `SetTransactionExtensionOrigin` via a new `TransactionExtension`. Don't subclass `SignedTransactionOrigin` or compose a custom `TransactionOrigin` from scratch.
5. **`major`** — Manual binary encoding of arguments is forbidden when `BinaryScale` / `autoEncodedArgs` covers the case.
6. **`major`** — Origin's `paysFees` flag is the source of truth at the caller. Don't second-guess. Fee estimation uses it; submission uses it.
7. **`major`** — Multi-extrinsic submission from the same `(chainId, accountId)` uses `ExtrinsicBuilderSequence` for nonce management. Don't hand-roll.
8. **`blocking`** — A verdict about a submitted attempt is written only through `DurableVerdictWriter` (recovery pass and submission watch alike). It is where a `FAILURE` is handed back to the row's policy; a second writer silently makes failures final again. (Starting an attempt and a policy's give-up are the executor's writes on rows waiting to be built.)
9. **`blocking`** — An `AsyncDurableSubmissionPolicy` rebuild consumes and mints exactly the assets registered for the row. A retry re-arms the same `DurableTxId`; the domain's rows, locks and completion rules keep applying to it.
10. **`major`** — `AsyncDurableSubmissionPolicy.canRetry` never reads the chain — it runs while a verdict is written. Whether a rebuild can still land is decided in `prepareSubmission`, which returns `GiveUp` to end it; a failed `Result` is retried after a backoff, never treated as a verdict.
11. **`blocking`** — `canRetry` distinguishes `DurableFailureKind`: an `EXPIRED` attempt may be rebuilt indefinitely, but a `DISPATCH_FAILED` or `REJECTED` one would repeat on the same effects and must be bounded (coinage: only while the window is open). Nothing else ends that loop. **Exception:** a policy whose every rebuild re-picks the effect's slot (`NotificationSlotSubmissionPolicy` picks a fresh period and seq per build) is not repeating the same effects and may retry every kind unbounded, provided `prepareSubmission` returns `GiveUp` once no slot can be had. Document the exception at the policy's `canRetry`.
12. **`major`** — A `TxCompletionOracle`'s `LedgerView` never contains `PENDING_SUBMISSION` rows: they have no attempt in flight and cannot have produced an effect.
13. **`major`** — Work common to one policy's transactions of one group (pinned blocks, proofs, per-extrinsic tokens) is done once per `prepareSubmission` call, not per transaction.
14. **`major`** — When a new identity-proof origin is added, wrap its extension in `SetTransactionExtensionOrigin` and expose it via the matching `*Origins` factory (e.g. `CoinageTransactionOrigins`). Composition, not inheritance from scratch.

## Seams

| Seam | What it does | When to extend it |
|---|---|---|
| `SetTransactionExtensionOrigin(signerSource, extension)` | Wraps a custom extension into a `TransactionOrigin` | New origin family |
| `TransactionExtension` (sdk interface) | Carries the explicit/implicit fields the runtime needs | New on-chain origin's proof-bearing data |
| Per-feature `*Origins` factory | Constructs origins for callers; injected via interface | New origin in a feature |
| `ExtrinsicService.submitExtrinsic { runtime, builder -> ... }` | Build + sign + submit + reconcile | All new submission flows go through this |
| `ExtrinsicBuilderSequence` | Auto-nonce iterator | Multi-extrinsic batches |
| `ChainConnectionRefCounter` | Connection-on-demand | Any background submission |
| `DurableTransactionService.schedule` | Registers unbuilt transactions (locks taken at commit); safe inside an enclosing DB transaction | A user-facing flow that must not wait for slow extrinsic construction |
| `AsyncDurableSubmissionPolicy` + `@SubmissionPolicyKey` | Builds scheduled rows and rebuilds proven-failed ones; `DurableSubmissionExecutor` runs one coroutine per (policy, group) | A domain whose transactions should be built later or retried with the same effects |

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| Submitting an extrinsic from background work without `ChainConnectionRefCounter` | blocking | `withConnectionEnabled(...)` |
| Sharing one keypair across roles | blocking | derive a separate keypair per role |
| Truncating proofs/batches to fit a cap | blocking | plan exactly |
| Manual binary encoding when `BinaryScale` works | major | `BinaryScale` + `autoEncodedArgs` |
| Origin's `paysFees` second-guessed at caller | major | trust the flag |
| Multi-extrinsic submission without `ExtrinsicBuilderSequence` | major | use the sequence |
| Inheriting from another origin's extension when composition via `SetTransactionExtensionOrigin` suffices | major | composition first |
| `getOrThrow()` on a chain `Result` | major | see `code/results-and-errors.md § getOrThrow` |
| `withSessionEnabled { awaitCancellation() }` for long-lived connections | major | use `requestConnectionEnabled` + `release()` |
| Marking on-chain state changes locally without a rollback path | major | architect plan must include rollback (this is a recurring risk, not a recipe) |
| Writing a durable status via `compareAndSetStatus` outside `DurableVerdictWriter` | blocking | route through the writer |
| A retry loop around `submit` that mints new outputs per attempt | major | register with a submission policy; the engine re-arms the same row |
| Polling chain state instead of subscribing | minor | `.observe()` |

## North star

- **RFC-0010 W3S Allowance.** Slot allocation for storage/transactions/PGAS goes through the allowance system. Use the high-level allocator interfaces; don't reach into pallets.
- **`storageN<T>(name)` reified storage entries** are canonical (`architecture/chain-integration.md`). Legacy `binding = ::bindXxx` is forbidden for new code.

## Canonical examples

- Classic signed transfer: callers of `SignedTransactionOrigin` in `feature/transfers/impl/`.
- Custom proof-bearing origin end-to-end: `AsPersonalAliasWithProof` in `feature/people/impl/.../data/signer/origins/extension/`.
- Multi-extrinsic batch with per-coin keypair: `RealCoinageTransferSubmissionUseCase`.
- Fee estimation differentiated by origin: `RealExtrinsicService.estimateFee`.

## Where new things live

| Concept | Goes in |
|---|---|
| New origin family | `feature/<X>/impl/.../data/signer/origins/` (factory + extension class) |
| Custom `TransactionExtension` | `feature/<X>/impl/.../data/signer/origins/extension/` (or `feature/transactions/api/.../data/origins/` if generic) |
| `*Origins` factory interface | `feature/<X>/api/.../domain/` |
| New runtime API binding | `chains/.../call/api/<Pallet>RuntimeApi.kt` |
