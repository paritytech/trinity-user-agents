# SSO / Sign-In Host

## Overview

Cross-app authentication for Polkadot ecosystem dApps and services. SSO is independent from chat and runs through its own service stack, even though both surface inside the chat tab.

## Key Components

### Services (in ServiceCoordinator)
- `signInHostCoordinator` — drives the sign-in host flow (consent prompts, session lifecycle, response signing)

### Related
- `KeyDerivation` package — derives the per-domain keypair used to sign SSO responses
- `RootEntropyManager.shared` — supplies root entropy via Keychain
- `StatementStore` — transport for incoming sign-in requests

## Architecture

```
External app / dApp
        ↓ (deeplink or statement-store request)
SignInHostCoordinator
        ↓
SignInHost module (consent UI)
        ↓
KeyDerivation (per-domain keypair)
        ↓
Signed response → originating app
```

## Wire Coding

When adding `Data` fields to SSO SCALE messages, check the counterpart codec first: SDK `Bytes()`
is compact-length-prefixed (Swift `Data`'s default ScaleCodable), while `Bytes(n)` / `[u8; N]` is
fixed-size raw bytes with no prefix — decode those with `readAndConfirm(count:)` and encode with
`appendRaw(data:)`. Mixing the two decodes without error on some inputs and corrupts every field
after; pin the byte-level layout with a test.

### Account derivation selector

Every wire field that picks an account inside a product subtree carries one selector type,
`Either<u32, [u8;32]>` (`ProductAccountSelector` in `Products`): `ProductAccountId.derivationIndex`,
the proof-context suffix, `PaymentTopUpSource.productAccount`, and
`AllocatableResource.smartContractAllowance`. SCALE is `0x00 ++ u32-LE` | `0x01 ++ 32 raw bytes`;
JSON is a number | `0x`-prefixed 32-byte hex string. Don't invent per-field index encodings —
reuse the selector.

### Wire contracts require pinned vectors

Cross-platform wire layouts (message enum indices, fixed-size fields, selector encodings,
derivation outputs) must be pinned with exact-byte vector tests shared with other platforms — round-trip
tests pass even when both directions are consistently wrong. Current pins live in
`ProductAccountSelectorCodingTests`, `ProductSubtreeMessageTests`, and the `KeyDerivation`
package tests.

## Hard Rules

1. **Per-domain key derivation** — each requesting origin gets its own deterministic keypair via `SharedSecretDerivationDomain`. Never reuse a domain across features.
2. **Explicit user consent** — every sign-in response requires a UI confirmation; no silent signing. Exception: product-subtree public key requests are consent-free by spec — the response carries only a public key; secret-material allocation (`AutoSigning`) still requires consent.
3. **SSO is not chat** — `signInHostCoordinator` does not depend on chat session state and must not be wired through `chatCoordinator`. Keep its services and module boundaries separate even when both surface in the same tab.

## Seams

| Seam                    | Where                                | When to touch                     |
|-------------------------|--------------------------------------|-----------------------------------|
| Sign-in coordinator     | `Common/Services/SignInHost*`        | SSO flow / session changes        |
| Consent UI              | `Modules/SignInHost/`                | Consent screen changes            |
| Key derivation domain   | `KeyDerivation` package              | New SSO surface (new domain)      |
