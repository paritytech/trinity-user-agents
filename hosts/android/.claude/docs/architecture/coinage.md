# Coinage Architecture

> **What this doc is:** rules and seams for coinage code. Vocabulary is a glossary, not a tutorial.
> **What this doc is NOT:** an explainer of how coinage works. Read `docs/design/coinage/native-interface.md` at the repository root and the code (`feature/coinage/**`) for that.

Coinage is the payment primitive: money is held as power-of-2-denominated coins and recycler entries derived from the user's mnemonic. The TrUAPI core owns the wallet: its coinage layer selects, builds, submits and reconciles every transaction, and keeps the wallet in its own database. The app reaches it through `NativeCoinage` (UniFFI) and adapts it to the app's use cases in `feature/coinage/impl/.../core/`.

---

## Glossary

- **`NativeCoinage`** — the core's wallet handle, from `TrUAPIHostRuntime.coinage()`; `CoreCoinage` (products api) hands it out, `CoinageCore` wraps it.
- **Cents** — the unit the core counts value in. `CoinageUnits` converts to and from the payment asset's planks with the core's `assetUnit()`.
- **Operation id** — the 32-byte id every app operation runs under: blake2b-256 of a stable label (`CoinageIds.of`, `CoinageExportId.of`). Starting an operation again under the same id joins it.
- **Export** — coins handed out of the wallet for a payment, named by a `CoinageExportId`. Provisional until committed (`CoinageHandoffCommit`).
- **Import** — a claim of coins a peer sent us, under the id `CoinageIds.of(claimGroupOf(messageId))`.
- **Hand-over** — the wallet the app's own engine kept until the core took over (installations with their index marks, unsettled chat payments), passed once in `CoinageConfig` by `CoreCoinageConfig`.
- **`CoinageInstallationId` / `CoinageKeyIndex`** — the RFC-0017 page and item a coin or entry was derived under; holdings rows carry them.
- **`CoinagePaymentProcessingExtension`** — the chat extension that follows a payment message's export or claim.
- **`ExternalPaymentService`** — host-API-driven payments out of the wallet (RFC-0006), as core offloads.

---

## Rules

1. **`blocking`** — Commit an export before its keys are durable anywhere else: a chat payment commits inside the message's save transaction (`onSaved`), a submitter payment before the submitter gets the keys. Release an export whose keys never left. Why: a peer holding keys the wallet still counts as its own is a double spend.
2. **`blocking`** — Derive every operation id from something stable about the operation (the chat message id, the product's request id, the top-up group). Why: a retry after process death must join the operation, not start a second one.
3. **`blocking`** — The app never builds coinage transactions or derives coin keys itself. Everything goes through `NativeCoinage`.
4. **`major`** — Each `NativeCoinage` stream is bridged once, through `CoinageCore.observe`, and cancelled with its collector. Polling the core is forbidden.
5. **`major`** — A spend that needs value still gaining privacy passes `confirmed = true` only after the user confirmed it.
6. **`major`** — TrUAPI products' payments (RFC-0006 top-up, payment, balance) are served by the core itself. The app's `ExternalPaymentService` serves only the app's own screens and the legacy product host API.
7. **`major`** — The user's privacy strategy reaches the core only through `setPrivacy`, applied by `CoreCoinageServiceStarter`.

## Seams (composition points)

| Seam | What it does | When to extend it |
|---|---|---|
| `CoinageCore` | The core handle, unit conversion and stream bridge | New use of `NativeCoinage` |
| `followOperation` / `followStarted` | An app operation's status as claim or top-up progress | New operation the app follows |
| `CoinageHandOverPayments` | Contributes unsettled payments to the hand-over | A new feature that holds coinage payments of its own |
| `CoinagePaymentProcessingExtension` template | The "chat follows a payment" extension shape | Any new chat extension that reacts to wallet events |

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| Making a payment's keys durable before committing its export | blocking | commit first (rule 1) |
| Random id for an operation that can be retried | blocking | derive it from a stable label (rule 2) |
| Reading coin state from chain or Room in a feature | blocking | use the coinage api, backed by the core |
| `getOrThrow()` on a coinage `Result` | major | see `code/results-and-errors.md § getOrThrow` |

## Canonical examples

- Chat send: `RealSendEnterAmountInteractor.sendCoinage` → `CorePrepareCoinageTransferUseCase.prepareMemo` → commit in `onSaved`.
- Claim: `CoinagePaymentProcessingExtension.claimStatuses` → `CoreClaimReceivedCoinsUseCase`.
- Payment status: `CoreCoinagePaymentStatusUseCase` (export status per coin).

## Where new things live

| Concept | Goes in |
|---|---|
| Adapter over `NativeCoinage` | `feature/coinage/impl/.../core/` |
| Cross-feature coinage use case | `feature/coinage/api/.../domain/usecase/` |
| Hand-over and registry config | `CoreCoinageConfig` (remote config `account_data_store_config`) |
