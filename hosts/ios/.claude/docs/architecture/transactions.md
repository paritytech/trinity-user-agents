# Transactions Architecture

## Overview

Extrinsic construction and submission for Polkadot/Substrate chains. Uses `ExtrinsicService` (external package) for building and signing, with `substrate-sdk-ios` for chain interaction.

## Key Components

### External Packages
- **ExtrinsicService** — extrinsic construction and submission
- **substrate-sdk-ios** — JSON-RPC, storage queries, signing

### App-Level
- `Common/Services/Extrinsic/` — extrinsic handling utilities
- `Modules/TransferAmount/` — transfer flow UI
- `Modules/TransactionResult/` — result display
- `Modules/SelectToken/` — token selection for transfers
- `Packages/XcmDefinition/` — XCM message definitions
- `Packages/XcmTransfer/` — XCM transfer operations

## Transaction Flow

1. User initiates transfer in TransferAmount module
2. Interactor builds extrinsic via ExtrinsicService
3. User confirms and signs (local key from KeyDerivation)
4. Extrinsic submitted via substrate-sdk-ios JSON-RPC
5. Result tracked and displayed in TransactionResult module

## Hard Rules

1. **Extension ordering matters** — base extensions first, custom overrides after. "Should be before custom extension since later one must override local one" (PR review)
2. **Use `fromScaleEncoded` for decoding** — prefer SDK decode functions over custom helpers
3. **Implement `Decodable` for payload types** — use protocol conformance and `map`, not manual JSON parsing
4. **Models must be generic where needed** — e.g., `TransactionPayload<Signer>` to support different signer types; avoid duplicating models
5. **Never fallback to raw bytes silently** — throw an explicit error when decoding expectations are not met
6. **Use `Data.toHex(includePrefix: true)`** for hex conversion — the `Data` wrapper is preferred over the underlying `NSData.toHexString` from NovaCrypto

## Seams

| Seam                        | Where                                   | When to touch                         |
|-----------------------------|------------------------------------------|---------------------------------------|
| ExtrinsicService config     | ExtrinsicService package                 | Extrinsic construction changes        |
| Submission strategy         | `ExtrinsicService(submitter:)`           | Validation / retry / fork-protection behavior |
| Transfer flow               | `Modules/TransferAmount/`               | Transfer UI/UX changes                |
| XCM definitions             | `Packages/XcmDefinition/`              | Cross-chain transfer support          |
| Transaction extensions      | `Common/Services/Extrinsic/`            | Custom extrinsic extensions           |
| Create transaction (HostApi)| Product host API bridge                  | Transaction creation for products     |

## Common Review Feedback

- Use existing SDK types and decoders instead of writing custom parsing
- Field ordering in models must match expected precedence
- Throw errors for unexpected data instead of silent fallbacks
- Use generics to unify SSO and host API transaction models
