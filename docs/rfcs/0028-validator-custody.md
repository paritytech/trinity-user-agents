---
title: "Host-custodied Validator capability"
status: draft
---

# RFC 0028 — Host-custodied Validator capability

## Summary

Expose the experimental Validator contract as an optional host capability for a PolkaVM validator product. The host owns keys, exclusive custody, durable signing guards and bounded recovery storage; the guest owns consensus, replay, state-transition execution, target validity and broadcast. This RFC records the existing prototype wire contract without standardizing its experimental allocation or claiming that a native backend ships with the shared runtime.

## Motivation

A validator must not lose its anti-equivocation history when a product restarts, changes accounts or is cancelled during signing. Giving a guest a seed or an arbitrary signing oracle makes these safety properties impossible to enforce at the host boundary. Typed custody allows the host to persist an intent before releasing a signature while leaving validator computation in the guest.

### Non-goals

- Running consensus, synchronizing peers, executing work packages or validating opaque recovery state inside the host.
- Key enrollment/import/export through the product API, arbitrary-byte signing, or a native validator fallback.
- Hardware-backed key protection, production qualification, guaranteed operation while a host is suspended, or proof that a signature was included in a finalized block.
- Adding JamPeerTransport, a byte feed, Chat or Seity to this feature layer.

## Approach

### Contract and ownership

The source of truth is [`api/validator.rs`](../../rust/crates/truapi/src/api/validator.rs), its [V1 payloads](../../rust/crates/truapi/src/v01/validator.rs), [versioned wrappers](../../rust/crates/truapi/src/versioned/validator.rs), the [runtime adapter](../../rust/crates/truapi/src/runtime/validator.rs) and `ValidatorPlatform` in [`platform.rs`](../../rust/crates/truapi/src/platform.rs).

Preserve prototype trait **112**, method ids, SCALE variant order and field order exactly:

| Method id | Method | Responsibility |
| --- | --- | --- |
| 0 | `open` | Acquire, read recovery/archive material, acknowledge replay, page or look up guards |
| 1 | `commit` | Sequential recovery/archive upload, checkpoint, witness supersession |
| 2 | `sign` | Durable typed GRANDPA intent and Ed25519 signature |
| 3 | `close` | Fence admission, drain and release custody |
| 4 | `vrf_output` | Typed Safrole eligibility output |
| 5 | `ticket_prove` | Bounded waiting-ring ticket proof |
| 6 | `entropy_source` | Typed `jam_entropy` signature |
| 7 | `seal` | Durable one-payload-per-slot block seal |
| 8 | `full_sign` | Durable assurance, guarantee, announcement or judgement |
| 9 | `audit_prove` | Initial/subsequent audit-selection proof |

The runtime supplies `ValidatorAuthority { product_id, account, execution_id }` from the trusted product connection and current account session. The guest supplies only an enrolled genesis when acquiring custody. Acquisition returns public identities, an opaque execution-local handle, enrolled parameter digest, generation, revision/head, recovery reference, history root/counts, role flags, acquisition time and quotas. The only declared protection class is `OsKeyringWrappedSoftware`: a software seed exists in trusted host memory; this is not a hardware claim.

### Permission and consent

`RemotePermission::Validator { genesis, account, execution }` is distinct from network permission. Its explicit SCALE discriminant is **6**, preserving the prototype while leaving index 5 unimplemented in this layer. It is checked through the product-scoped permission service on acquisition and subsequent admitted operations; first-party remote-permission defaults never grant custody. Execution identity includes a process-random component rather than relying on a restart-repeating counter. A manifest declaration, local product identifier or network grant does not authorize custody.

The trusted platform adapter must additionally bind the verified artifact and live account to this execution and obtain fresh artifact consent. The runtime authority struct does not itself carry an artifact digest: artifact verification and consent are platform obligations, not something the guest can assert. Authoring requires the acquired immutable authoring role (role 3); full duties require explicit role-4 consent and authoring. Parameter bytes must hash to the enrolled digest. Vote-only ownership refuses authoring; authoring does not imply full-duty permission.

### State machine: acquire → ready → sign/commit → close

1. **Acquire:** lock an already enrolled identity. A competing owner or concurrent runtime operation receives `Busy`; acquisition is not enrollment. Validate the acquired handle, genesis, role consistency and bounded metadata before exposing ownership.
2. **Recover, then Ready:** read the latest complete recovery blob and retained guards, replay/revalidate in the guest, then acknowledge the exact revision/head using `open::Ready`. This acknowledgement is not a host proof of consensus. Retained pages contain GRANDPA intents followed by seals; full-duty history is available only by typed lookup. Archive objects are immutable content-addressed history, not live recovery witnesses.
3. **Upload:** `Begin`/`Chunk`/`Finish` declare a complete Blake2b-256 reference, append sequential chunks, then verify size/hash and durably store the blob and directory. Archive upload has separate variants and quota accounting. Partial uploads are not signable recovery. Upload completion alone is not a journal checkpoint or signing intent.
4. **Sign:** after replay readiness, submit an exact expected revision/head, typed duty and already complete recovery reference. The backend must persist the guard and required durable-head barriers before releasing a signature. GRANDPA stages are prevote (0), precommit (1), proposal (2), with set/round, target, posterior state root and slot. Seals bind a canonical decoded payload to a consumed slot. Full-duty guards distinguish assurance anchors, guarantees by report, announcements by canonical message hash and judgements by report/validity. Conflicting choices refuse; stale expected heads refuse. The host does not decide whether the guest's target or recovery state is valid.
5. **Commit:** `Checkpoint` compare-and-appends an already complete recovery reference. `Supersede` binds the ready guest's assertion that one complete checkpoint covers every retained duty to the exact expected head; the backend durably rebinds witness references before reclaiming old blobs. It must retain signing choices, including consumed-slot guards. Supersession is not permission to forget conflicts.
6. **Close:** stop admitting work, drain accepted persistence/signing and any owned peer transport, reap the helper, then release exclusive ownership. Closing remains possible after revocation. Cancellation or ambiguous completion fences the owner rather than treating dropped work as a completed release.

`vrf_output`, `ticket_prove`, `entropy_source` and `audit_prove` use typed domains and role/parameter checks, but do not use the revision/head-bearing durable-intent request shape. An audit-selection proof is not a completed audit.

Account/session changes, permission revocation and execution teardown revoke admission. Results are checked against live authority after callbacks, so a signature completed after revocation is not delivered. An `Unknown` private callback diagnostic becomes `Poisoned` and revokes ownership; diagnostics must not reach the guest. The platform must retain ownership or poison it when drain completion is ambiguous.

### Exact bounds

These are contract/runtime bounds, not benchmark results or a guarantee of available disk:

| Resource | Bound |
| --- | --- |
| Complete recovery/archive reference | 1–67,108,864 bytes (64 MiB) |
| Read request | 1–262,144 bytes (256 KiB); offset cannot exceed declared length |
| Upload chunk | 1–262,144 bytes, sequential offsets |
| Canonical seal payload | 1–262,144 bytes |
| Encoded authoring/full-duty parameters | 1–1,024 bytes; digest must match enrollment |
| Ticket waiting ring | 1–1,023 public keys; index must be inside ring |
| Retained page | At most 64 intents and seals combined |
| Acquired handle | 1–256 bytes |
| Decimal `u64` fields checked by runtime | 1–20 ASCII digits, no leading zero except `0`, parseable as `u64` |
| Announcement reports | 1–65,535 entries, unique `u16` core indices; preserve protocol order |
| Runtime admission | One operation at a time per product runtime; contention returns `Busy` |

The native backend must enforce canonical parameters, enrolled core/attempt limits, sequential transfer state, durability and conflict rules. Aggregate storage, history and archive quotas are reported by acquisition as decimal byte counts; there is no universal fixed 512 MiB quota in this API. The behavioral test fixture's 536,870,912-byte values are not a production default. Upload admission is bounded by the backend's sole in-progress upload contract and quotas; exhausting capacity must fail closed rather than discard signing history.

### Threat model

An untrusted or compromised guest may forge handles, request another account/genesis, replay stale heads, submit conflicting duties, provide malformed or incomplete recovery, exhaust quotas, or race permission changes. Runtime authority, scoped permission checks, exact-head comparisons, bounded inputs and durable backend guards address these boundaries. A compromised guest can still choose a bad consensus target within an authorized typed domain: custody is not a replacement for correct guest execution or artifact review.

Crashes, failed persistence, private bridge decode failures and cancellation may leave accepted work without a delivered response. Reopening must recover durable history, never infer that no signature was produced from a missing response. Exclusive custody and fail-closed poison handling belong to the native backend as well as the runtime fence. This design does not protect against a compromised host/OS or guarantee safety if an operator clones or rolls back private custody state outside the backend's protections.

Public identities, acquisition time, history counts and recovery bytes are intentionally visible to the authorized guest. Seeds, private bridge diagnostics and filesystem paths are not. Content addressing verifies bytes, not availability or consensus validity.

### Platform matrix

| Platform | Status and obligation |
| --- | --- |
| Shared Rust runtime | Optional `ValidatorPlatform` seam, typed dispatch, permission/lifetime enforcement; no native custody implementation implied |
| Epoca native shell + protected helper | Integration target: shell verifies artifact and obtains consent; helper owns enrollment, keys, exclusive custody, durable journal/storage and draining. Shipping/qualification is separate from this RFC |
| Other native/mobile hosts | Unsupported unless a conforming protected backend is explicitly installed; no inferred support from generated bindings |
| Browser/WASM host | Generated and tested optional `validator` host-callback namespace; the WASM runtime installs the adapter when host JavaScript provides it. Without it, acquisition returns domain `Unsupported`. No protected custody backend is supplied by this layer |
| Browser/native bridge backend | The callback plumbing exists, but enabling protected custody still requires a reviewed backend with artifact-bound consent, exclusive ownership, persistence and crash/revocation semantics; product JavaScript must not own keys |

### Relation to JamPeerTransport

JamPeerTransport is a separately layered capability (prototype trait 111) with its own permission and transport lifecycle. Validator trait 112 neither grants network access nor introduces its implementation, byte-feed machinery or transport dependencies. A native integration that couples custody to a peer identity must drain that owned transport before releasing custody; this is an adapter lifecycle obligation, not permission to merge the transport feature downward. Consensus messages and broadcast remain guest responsibilities through separately authorized transport.

## Base-layer migration prerequisite

The current base consolidates the former `truapi-server` and `truapi-platform` crates into `truapi`; this port uses its existing execution-local `ConnectionAdapters` API rather than adding the prototype's parallel constructor.

The prototype also expanded generic product identity policy for strict lowercase `UUID.epoca-local` installations. That policy is **not included here**: current-base fixtures use `validator.paseo`. A locally installed Epoca consumer using the prototype identity needs a separate base-layer change before this layer can replace its host build:

- Former `truapi-platform/src/lib.rs`: `is_epoca_local_product_identifier`, the corresponding `normalize_product_identifier` branch, `RuntimeConfigValidationError::InvalidProductId` wording and `local_install_identity_is_strict_and_never_a_dotns_authority` test. Their current home would be `truapi/src/platform.rs`.
- Former `truapi-server/src/runtime/product_manifest.rs`: return no remote root manifest for a strict local-install identity; its current home is `truapi/src/runtime/product_manifest.rs`.

This is product-identity and manifest-resolution policy, not validator authorization. Accepting the identity must not grant account, artifact, remote-operation or custody authority. No change to the base branch is made by this RFC.

This layer also fixes generic code generation to honor explicit enum discriminants. That fix could flow into #540 independently of Validator. The v01 Validator prototype remains unchanged except for a `clippy::large_enum_variant` allowance on `HostValidatorOpenResponse` (with its explanatory comment); variants and field order remain byte-identical.

## Trade-offs

- A trusted helper and durable journal increase host complexity, but exposing keys or generic signing would remove the safety boundary entirely.
- Opaque recovery keeps consensus out of the host but makes correctness depend on a trusted, reviewed guest artifact and honest replay/supersession assertions.
- Fail-closed cancellation and poisoned state can reduce availability. Automatically releasing ambiguous custody or silently deleting guards would trade safety for liveness.
- Experimental trait 112 and the complete V1 payload layout are preserved for existing consumers; future standardization/allocation is a separate proposal, not a renumbering in this port.
- Revocation is permanent for the current execution, including after a later permission grant; recovery requires a new execution.
- Permission decisions are stored per execution. This prevents grant reuse across executions but accumulates durable permission records; record lifecycle/retention is not implemented here.
- Product-facing generic permission requests reject Validator scopes without prompting or storing a decision. Only trusted custody acquisition derives the account, genesis and execution scope.

## Testing and qualification

[`validator_contract.rs`](../../rust/crates/truapi/tests/validator_contract.rs) exercises unsupported acquisition through the real product dispatcher. [`validator_tests.rs`](../../rust/crates/truapi/src/runtime/validator_tests.rs) uses a controlled custody adapter to exercise scoped administrative revocation, in-flight result fencing, role/parameter checks, invalid announcements and foreign handles. These are runtime contract tests, not native durability, keyring, full-validator or live-network qualification.

Native integration additionally requires crash/restart and storage-failure qualification, duplicate-owner exclusion, exact-artifact consent, durable conflict history, recovery replay, quota exhaustion and drain/revocation tests. Sustained end-to-end validator duties and independently confirmed network inclusion remain separate acceptance criteria; this RFC makes no claim that they have run.
