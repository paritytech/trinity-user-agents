# Statement-Store Communication

> **What this doc is:** rules and seams for off-chain peer messaging via the substrate statement-store pallet.
> **What this doc is NOT:** an explainer of crypto primitives. Read `feature/statement-store/**` for that.

Shared off-chain message bus used by SSO pairing and chat. End-to-end encrypted, sender-authenticated, slot-allocated.

---

## Glossary

- **`Statement`** = `(body, proof)`. `Body` carries `topic1..4`, `channel`, `expiry`, `data`. `Proof` is Sr25519 sig + signer public key.
- **Topics** — subscription filters. **Channel** — secondary routing key within a topic.
- **`StatementStoreService`** — submit / fetch / subscribe. Submission retries 10× with 2-s backoff.
- **`StatementStoreSlotAllocator`** — per-period slot lifecycle. LRU eviction after `StmtStoreReplacementCooldown`. Required before first submit.
- **`CommunicationSession`** — bidirectional shared abstraction. Polling + encryption + verification + state machine. One session per `(feature, peer pair)`.
- **`SharedSecretDerivationDomain`** — per-feature label that prevents one ephemeral keypair seeding multiple features' channels.
- **`CommunicationEncryption`** — ECDH + HKDF-SHA256 + AES.
- **`StatementStoreMessageProver`** — Sr25519 signer over a deterministic-ordered body.

---

## Rules

1. **`blocking`** — Every new feature using statement-store declares its own `SharedSecretDerivationDomain` value. Never reuse another feature's domain.
2. **`blocking`** — Submission requires an active slot. New code must allocate via `StatementStoreSlotAllocator.allocate(...)` before the first submit, and document *when* in the architect plan (eager / lazy on first message).
3. **`major`** — Bidirectional flows use `CommunicationSession`. Hand-rolled polling + decrypt + verify loops are forbidden.
4. **`major`** — One `CommunicationSession` per `(feature, peer pair)`. Sessions are **not** multiplexed across features.
5. **`major`** — Wire payloads are `@Serializable` + `BinaryScale`. No JSON, no hand-rolled binary.
6. **`major`** — Each feature wraps payloads in a feature envelope (`SignalingEnvelope` shape) — never publish raw inner types. Forward-compatibility comes from the envelope.
7. **`major`** — `maxStatementSize` is picked deliberately per feature and validated against worst-case message size. No statement fragmentation across multiple statements.
8. **`major`** — One-shot statements (no session, like SSO handshake) sign via `StatementStoreMessageProver` and submit via `submitStatement` / `submitStatementOnce`.

## Seams

| Seam | What it does | When to extend it |
|---|---|---|
| `StatementStoreService` | submit / fetch / subscribe | Never extend directly; use `CommunicationSession` for bidirectional, or one-shot prover for handshakes |
| `CommunicationSessionCreator` | Builds a session for a `(local, remote)` pair | New bidirectional flow |
| `SharedSecretDerivationDomain` | Per-feature key isolation | Each new feature adds a value |
| `StatementStoreMessageProver.Factory` | Per-signing-context provers | New signing context (e.g. a non-`MetaAccount` signer) |
| `StatementStoreSlotAllocator` | Slot lifecycle | Don't reach into the pallet; use this |

## Anti-patterns

| Anti-pattern | Severity | Fix |
|---|---|---|
| Reusing another feature's `SharedSecretDerivationDomain` | blocking | declare your own |
| Submitting without allocating a slot | blocking | `slotAllocator.allocate(...)` first |
| Reusing a peer's session pin for an unrelated use case | major | unique pin per feature |
| Hand-rolled polling + decrypt + verify loop | major | use `CommunicationSession` |
| Multiple consumers sharing one session | major | one session per `(feature, peer pair)` |
| Raw JSON / hand-rolled binary on the wire | major | `@Serializable` + `BinaryScale` |
| Message fragmentation across multiple statements | major | redesign to fit `maxStatementSize` |
| `getOrThrow()` on `submitStatement` result | major | see `code/results-and-errors.md § getOrThrow` |

## North star

- **RFC-0010 W3S Allowance** changes how slots are accounted for. Use the high-level `StatementStoreSlotAllocator` interface; don't reach into pallets.
- **JAM-codec adoption** (RFC-0002 alignment): wire payloads may move from SCALE to JAM. The `@Serializable` boundary insulates features from the change.
- **Native push from the chain** would obsolete the polling that `CommunicationSession` does internally. Treat polling as a temporary implementation detail behind the session.

## Canonical examples

- One-shot statement (no session): SSO `SsoHandshakeRepository.submitHandshakeAnswer`.
- Bidirectional session: `SyncPeerChannelSignaling` (`feature/device-sync`) driving a `CommunicationSession`.
- Topic discovery + encrypted payload: `ChatRequestTransport.submitChatRequest` (three topics: day-keyed, full, session-specific).
- Slot management: `RealStatementStoreSlotAllocator.allocate` with LRU eviction.

## Where new things live

| Concept | Goes in |
|---|---|
| New `SharedSecretDerivationDomain` value | `feature/statement-store/api/.../SharedSecretDerivationDomain.kt` (the enum/sealed) |
| Feature envelope payload | `feature/<X>/impl/.../data/scale/<X>Envelope.kt` (or in a sibling package) |
| Feature-specific topic derivation | `feature/<X>/impl/.../<X>Protocol.kt` (constants live next to the protocol object) |
