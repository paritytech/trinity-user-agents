# Statement Store Communication

## Overview

Off-chain peer messaging via Substrate statement store. Statements carry encrypted payloads between peers, enabling chat messages, session negotiation, and feature-specific data exchange.

## Key Components

### Package
- **StatementStore** (`Packages/StatementStore/`) — statement storage, retrieval, and lifecycle management

### Related
- **MessageExchangeKit** — encrypted message protocol built on top of statement store
- **ChatStorage** — persistence for received messages

## Encryption

All payload encryption uses X25519 key agreement plus IETF ChaCha20-Poly1305 (chat-spec
RFC-0004) — the single primitive pair app-wide (chat, MDS fan-out, device sync, SSO
handshake, HOP attachments):

- Public keys are raw 32-byte X25519 keys on the wire; the on-chain identity record keeps
  a 65-byte container (`0x00` type byte + key + zero padding, `Chat.OnChainEncryptionIdentifier`).
- **Local vs remote key representations are distinct types — never raw `Data`.**
  `Chat.PublicKey` holds only the local raw 32-byte public key;
  `Chat.OnChainEncryptionIdentifier` holds the remote on-chain container. Raw container
  bytes may appear only at the wire boundaries that carry them: `ConsumerInfo.identifierKey`
  and extrinsic call params (`SignUpWith*Call.identifierKey`, `RegisterUsernameParameters`).
  Everything in between passes the typed values and converts via
  `.localPublicKey` / `.scaleEncoded()` at the boundary.
- Symmetric keys derive from the DH shared secret via HKDF-SHA256 (empty salt/info, 32 bytes).
- AEAD framing is `nonce(12) ‖ ciphertext ‖ tag(16)` (CryptoKit combined form), no AAD.
- Small-order peer keys abort key agreement (CryptoKit throws on an all-zero shared secret);
  the negative tests in `polkadot-appTests/Crypto/` pin this along with the RFC 7748/8439
  vectors.

## Statement Model

A statement consists of:
- **Body** — encrypted payload data
- **Proof** — cryptographic proof of authorship
- **Topics** — routing metadata
- **Channel** — communication channel identifier
- **Expiry** — statement TTL
- **Data** — raw payload

## Communication Session

Bidirectional communication sessions between peers:
- Statement store slot allocation
- Shared secret derivation per feature domain
- Binary payloads (not JSON) for wire format

## Statement Store Slot Allocation

`StatementStoreSlotAllocator` (Individuality) funds a statement account for the current period, chosen by
`StatementStoreSlotInfoProvider`:

- **Free slot** → `set_statement_store_account`, authorized by a ring proof (`AsResources(RegisterStatementStoreAllowance)`).
- **Occupied slot** → one of two strategies, selected per allocation by `StatementStoreRuntimeCapabilities`
  - *old runtime*: `set_statement_store_account` with the ring proof, allowed once the entry's
    `since` plus the runtime's `get_stmt_store_replacement_cooldown` has elapsed (`LegacyEvictionPolicy`, chain time).
  - *new runtime*: `replace_statement_store_account`, authorized by a plain sr25519 signature of the
    account currently holding the slot (`AsResources(ReplaceStatementStoreAccount)`), with the `RestrictOrigins`
    extension enabled. The call carries the slot alias and the entry's `since` timestamp as replay protection.

Eviction candidates are the occupied slots whose holder does not outrank the caller (accounts without a local
`AllowanceRecord` rank as normal), ranked by `(priority, since)` oldest first. Only the replacement strategy
additionally requires a `signerKeyId`, because it must sign as the holder. The chain
rate-limits each slot holder through the origin-restriction pallet, and the recovery window is not
readable from storage, so the allocator dry-runs the signed replacement with
`TaggedTransactionQueue_validate_transaction` (`SlotReplacementValidator`): `Payment` or
`Custom(228)` means "cooling down, try the next candidate", any other invalid reason is
`replacementRejected`, and an inconclusive answer proceeds to submission. A stored key that no longer
derives its holder is skipped like a keyless row. Every evictable candidate cooling down raises
`noSlotsAvailable`; nothing signable to evict raises `noEvictableSlots`.

## Hard Rules

1. **SharedSecretDerivationDomain must be per-feature** — never reuse derivation domains across features
2. **Binary payloads on wire** — use SCALE-encoded binary, not JSON
3. **Feature envelopes** — each feature wraps its data in a typed envelope
4. **Slot allocation is managed** — use `StatementStoreSlotAllocator` for proper slot lifecycle
5. **Statement-store allocation takes a `StatementStoreAllowanceHolder`** — `.local(accountId:signerKeyId:)`
   when this installation holds the key, `.remote(accountId:)` otherwise (desktop-held device accounts,
   other installations of the same seed). On new runtimes only `.local` rows are ever evicted;
   old runtimes evict either, since the ring proof needs no holder key

## Seams

| Seam                     | Where                            | When to touch                      |
|--------------------------|----------------------------------|------------------------------------|
| Statement models         | `Packages/StatementStore/`       | Statement structure changes        |
| Slot allocation          | `Packages/StatementStore/`       | Allocation strategy changes        |
| Message protocol         | `Packages/MessageExchangeKit/`   | Messaging protocol changes         |
| Chat integration         | `Packages/ChatStorage/`          | Chat message persistence           |
