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
- Statement store slot allocation (per-period LRU)
- Shared secret derivation per feature domain
- Binary payloads (not JSON) for wire format

## Hard Rules

1. **SharedSecretDerivationDomain must be per-feature** — never reuse derivation domains across features
2. **Binary payloads on wire** — use SCALE-encoded binary, not JSON
3. **Feature envelopes** — each feature wraps its data in a typed envelope
4. **Slot allocation is managed** — use `StatementStoreSlotAllocator` for proper slot lifecycle

## Seams

| Seam                     | Where                            | When to touch                      |
|--------------------------|----------------------------------|------------------------------------|
| Statement models         | `Packages/StatementStore/`       | Statement structure changes        |
| Slot allocation          | `Packages/StatementStore/`       | Allocation strategy changes        |
| Message protocol         | `Packages/MessageExchangeKit/`   | Messaging protocol changes         |
| Chat integration         | `Packages/ChatStorage/`          | Chat message persistence           |
