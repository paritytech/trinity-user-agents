# Coinage Architecture

## Overview

Coinage is the payment primitive for the Polkadot app: the wallet's coins and recycler entries, chat payments, top-ups and payments out. The wallet is the Rust core's coinage layer, reached through the shared `TrUAPIHostRuntime` (`NativeCoinage`, contract in `docs/design/coinage/native-interface.md` at the repository root). The app holds no coins, keys or transactions of its own.

## Key Components

### Package
- **Coinage** (`Packages/Coinage/`) — `CoinageServicing` and its core-backed `CoinageService`, `CoinageCore` (the handle, cents↔planks via `asset_unit`, observer→stream bridge), and the models the screens use (`CoinageBalance`, `CoinageHoldings`, `TransferMemo`, `BackupProgress`, payment statuses).

### Module
- **Coinage** (`Modules/Coinage/`) — `CoinageRuntimeConfigFactory` (the runtime's `CoinageConfig`: registry contract and hand-over), `CoinageTransferMonitor` (chat payment statuses), `CoinageBackupSyncService`, the privacy-strategy and recovery-acknowledgement stores.

### Services (in ServiceCoordinator)
- `coinageService` — the wallet; `start()` keeps the core's privacy preset the user's choice
- `coinageBackupSyncService` — the wallet's "balance restored" card, from `BackupProgress`

## Operation ids

Every operation the app starts has a 32-byte id (`CoinageOperationId`), BLAKE2b-256 of a stable label, so a retry joins instead of starting twice:

| Operation | Label |
|---|---|
| Chat payment sent in message M (export) | `chat-send:M` |
| Claim of the payment received in message M (import) | `chat-claim:M` |
| Taking back a chat payment whose message was never saved | `chat-reclaim:M` |
| W3S merchant payment P (export) | `w3s:P` |
| External payment (offload) | `external:<productId or native>:<paymentId>` |
| Product top-up | `top up:<productId>:<paymentId>` |

One-off operations (wallet funding, deep search) use random ids.

## Paying

The transport commits the export right before it makes the keys durable anywhere (`TransferSubmitting.sendTransfer`'s `commit`), so a committed export is never one whose keys nobody can hold. A send that fails before the commit releases the export; one that fails after it reclaims the coins (`PreparedTransfer.abandon`). Value still gaining privacy is spent only with `confirmed: true`, after the user agreed.

## Hand-over

The first time the core starts it takes over the wallet the app's own engine kept (`CoinageRuntimeConfigFactory`): every installation in `CDCurrentInstallation`/`CDCoinageInstallation` with its high-water marks, every outgoing chat payment and W3S payment not settled, and every incoming chat payment not claimed. The core ignores the hand-over once its store exists, so it is built on every runtime start. The engine's Core Data entities stay in the model only for this.

## Seams

| Seam | Where | When to touch |
|---|---|---|
| Core calls and streams | `Packages/Coinage/Sources/Core/` | Native interface changes |
| Runtime config and hand-over | `Modules/Coinage/HandOver/` | Registry config, what the app hands over |
| Chat payment status | `Modules/Coinage/Services/CoinageTransferMonitor.swift` | Message bubble states |
| Wallet card | `Modules/AssetDetails/Model/CoinageBreakdownFactory.swift` | Holding rows, ordering, wear |
