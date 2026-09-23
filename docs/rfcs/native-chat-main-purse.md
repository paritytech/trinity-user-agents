---
title: "Product-owned native Chat and main-purse payment authority"
owner: "@replghost"
status: draft
---

# RFC — Product-owned native Chat and main-purse payment authority

## Summary

Products own ordinary native Chat: invitations, conversation state, subscriptions, delivery retries, and
acknowledgments. The Host retains non-exportable identity/device keys, authenticated peer and payment eligibility state,
outgoing payment secrets, trusted spend approval, exclusive main-purse wallet service ownership, and durable payment
recovery. Incoming native payment secrets may enter the product, which persists them privately and imports them through
`payment.top_up(Coins)`.

This proposal is **draft, not approved**. It accompanies
[host-rust-core #709](https://github.com/paritytech/host-rust-core/pull/709) and its separate Chat product integration;
inclusion here is neither release availability nor deployment evidence. It does not belong in the generic PolkaVM base.

**Transitional payment interface.** Native `CoinageSend` remains supported. The intended replacement is
[RFC 0017](0017-coinage-payment.md) receivables, encrypted cheques, and deposits using `MAIN_PURSE`, not a separate Chat
purse. Native memos are not RFC 0017 cheques, and this implementation does not supply that RFC's general payment API.

## Motivation

Ordinary Chat orchestration does not require a Host-owned conversation actor. Outgoing payments do require a stronger
boundary: a compromised product must not obtain outgoing bearer secrets, select wallet inputs, authenticate its own
recipient keys, or approve a debit from the user's ordinary balance.

Incoming bearer secrets are deliberately product-visible. A compromised product could redirect them before deposit; this
design does not claim otherwise. Once the wallet accepts an import, its durable claim plan and recovery are Host-owned.
A reload before durable recipient custody can interrupt automatic claim/retry. It is not proof of permanent fund loss: a
sender retaining the keys may still control coins not claimed elsewhere.

| Responsibility                                                            | Owner                        |
| ------------------------------------------------------------------------- | ---------------------------- |
| Invitations, ordinary messages, reactions/edits, conversation history     | Product                      |
| Statement subscriptions, submission, ordinary outbox/retries, native ACKs | Product                      |
| Wallet identity and installation device secrets                           | Host only                    |
| Authenticated peer admission/revocation and outgoing payment eligibility  | Host                         |
| Outgoing main-purse selection, consent, memo, reservations and settlement | Host only                    |
| Incoming bearer keys before import                                        | Product, privately persisted |
| Accepted incoming claim plans, destination keys and finalized evidence    | Shared Host wallet           |
| Trusted file selection/export, private file tickets, HOP retrieval        | Host capability              |

The Host device secret cannot be exported merely because ordinary Chat moved into the product. Native outgoing
ciphertext uses that device's key agreement; exporting it would expose outgoing payment memos. `Open` therefore accepts
an authenticated complete external statement, not an arbitrary ciphertext/key pair. Own-signer reflections, invalid
routes, and unadmitted senders must be rejected before any plaintext is returned.

## Detailed Design

## API and compatibility

The source of truth is:

- [`api/account.rs`](../../rust/crates/truapi/src/api/account.rs): `Account::product_device_chat`, trait **2**, method
  **12**;
- [`versioned/account.rs`](../../rust/crates/truapi/src/versioned/account.rs): request/response envelope **V2**, error
  **V1**;
- [`v03/account.rs`](../../rust/crates/truapi/src/v03/account.rs): the narrow request and response payloads;
- [`v02/account.rs`](../../rust/crates/truapi/src/v02/account.rs): reused public metadata and the decode-only legacy
  view;
- [`v01/payment.rs`](../../rust/crates/truapi/src/v01/payment.rs): the existing generic top-up request and errors.

The old method-12 V1 request is recognized only to return `CallError::unavailable()`. No high-level actor operation is
emulated, and no V1 response is emitted. Retired response versions do not renumber V2's SCALE discriminant. Account
method **11** remains retired. The new encrypted SSO Chat operation is **V3, tag 7**; older operations are rejected.
Generic encrypted SSO top-up messages use appended tags **28/29**, without reusing older tags.

Guest, SDK, generated codecs, core, and consuming artifacts must be upgraded together. A package's broad version number
is not capability negotiation. Unsupported methods and versions must fail explicitly, never return empty success or fall
back to raw guest-owned device crypto.

### Requests

`Id32` below means `[u8; 32]`. All operations are request/response calls, not Host-owned Chat subscriptions.

| Variant               | Fields                                                              | Contract                                                                                                                  |
| --------------------- | ------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `Initialize`          | none                                                                | Restore public device/security state, opaque pending handoffs and any legacy migration. No spend authority.               |
| `Bind`                | `username: String`                                                  | Independently resolve and bind the peer identity and root Chat key on the configured network.                             |
| `Prepare`             | `peer_identity: Id32`, `route`, `plaintext: Vec<u8>`                | Validate native product-authored content and return signed ciphertext. No delivery.                                       |
| `Open`                | `statement: SignedStatement`                                        | Authenticate the external native signature, direction, route and device membership before returning allowed plaintext.    |
| `ContinueOpen`        | `open_id: Id32`, `cursor: u32`                                      | Replay or continue a bounded authenticated HOP handoff.                                                                   |
| `SendPayment`         | `peer_identity: Id32`, `request_id: String`, `amount_cents: u64`    | Propose one recipient-bound main-purse debit, subject to trusted Host review.                                             |
| `PaymentStatus`       | `operation_id: Id32`                                                | Read this product's durable payment state without authorizing another debit.                                              |
| `ReconcilePayments`   | none                                                                | Reconcile existing payment custody and opaque handoffs; do not activate an unrelated allocator for ordinary Chat polling. |
| `PaymentDenomination` | none                                                                | Read trusted configured raw chain units per native Coinage cent; no balance, inventory selection, or transfer.            |
| `PrepareAttachments`  | `peer_identity: Id32`, `request_id: String`, `text: Option<String>` | Trusted file selection and private preparation; the product transports the resulting ciphertext.                          |
| `OpenAttachment`      | `attachment_id: Id32`                                               | Resume a private download and present/export through trusted Host UI.                                                     |
| `CommitMigration`     | `migration_id: Id32`                                                | Acknowledge durable product import of the complete frozen legacy view and ordinary ciphertext.                            |
| `ContinueState`       | `state_id: Id32`, `cursor: u32`                                     | Read another immutable page of the original response's public metadata.                                                   |

Routes are `Invitation`, `Identity`, and `Device`. Invitation plaintext uses the native request-message encoding;
identity/device plaintext uses tagged native request/response encoding. The product chooses native message IDs and
timestamps. The Host validates legal own-device lifecycle advertisements and authenticates peer-controlled changes.
`Prepare` must not become an outgoing payment-memo or private file/history capability injection path.

Native identifiers, content, recipients, storage and nested history remain bounded. Invalid input is rejected rather
than treated as an opaque signing/decryption tunnel. Capacity exhaustion must not evict live custody or idempotency
commitments.

### Responses and paging

Responses contain public `device` and authenticated `peers`, optional `binding`, authenticated `opened` frames,
`prepared` ciphertext, `payments`, and `rich_messages`. They also carry migration and paging metadata and an optional
`coinage_cents_unit` for the explicit denomination request. No field is a wallet balance.

Each prepared delivery contains the signed statement, peer identity, native request ID, and `requires_ack`. Response
frames must not generate ACK-of-ACK traffic. Optional `client_request_id` preserves the old caller/native ID mapping
when transferring pending legacy sends. Incoming opened plaintext may contain payment keys and lifecycle messages; its
debug representation is redacted and owned secret-bearing buffers zeroize.

- `ContinueOpen` pages retain the **original** native request ID. Each page is immutable, authenticated and replayable;
  reading does not destructively advance a cursor. Pages contain at most 256 KiB of native frames and 256 frames.
- `ContinueState` pages contain at most 512 KiB of public metadata. Large legacy history groups split only at whole
  native-frame boundaries, repeating their peer/direction/request identity. Consumers append/deduplicate their messages.
- Direct operation results (`binding`, `opened`, `open_page`) are not retained in metadata snapshots or repeated by
  `ContinueState`. Products must checkpoint these results as well as metadata continuation state.
- State snapshots are scoped to product and wallet session. A new response may replace a snapshot; stale IDs fail
  explicitly and the product retries its original durable operation, not a newly identified payment.
- The guest retains its 1 MiB frame bound. Its short generated correlation IDs fit the reserved envelope space.

Migration fields include the frozen legacy public view, `migration_id`, and original native invitation request IDs. All
pages must reach durable product storage before `CommitMigration`; a display-cache limit is not permission to truncate
migration input.

The domain error remains `HostProductDeviceChatError::V1`, including `NotConnected`, `AccessNotGranted`, `UserRejected`,
`AllowanceRequired`, `PeerNotReady`, `OperationConflict`, `InvalidRequest`, `InvalidStatement`, `RecipientNotFound`,
`InsufficientBalance`, `StorageUnavailable`, `NetworkUnavailable`, `OperationNotFound`, and `AttachmentsUnavailable`.
Errors and diagnostics must not include keys, decrypted memos, private file credentials, or raw secret-bearing backend
errors. Failure or timeout is not payment cancellation.

## Authentication and permissions

Calling-product identity comes from the Host connection/SSO context, not caller labels. Device state is scoped to
wallet, network, product and installation; the allocator and imported source identity are wallet/network-wide. Handles
and continuation IDs must not cross these boundaries.

Chat authority is distinct from username disclosure. Pure `Bind`, `Prepare`, and `Open` do not require Host
`StatementSubmit`; the product separately requests submission permission and the device's allowance. Uploads require
`PreimageSubmit` and the existing Bulletin allowance. None is a spend grant.

The Host verifies native invitations, peer identity proofs and device membership. Outgoing payments additionally require
an active keyed peer device to acknowledge the legacy-device revocation update. Only eligible devices enter the payment
envelope; an offline advertised device need not block an eligible one. Payment ACKs must come from a recipient included
in the committed envelope. Authenticated roster changes invalidate eligibility until re-established.

Every new outgoing debit requires trusted signing-Host `MainPurseChatPaymentReview`: authenticated product and
recipient, Host-resolved username when available, exact `amount_cents`, approved `max_debit_cents`, chain genesis,
configured Coinage instance and immutable operation ID. Guest UI, auto-sign policy and generic signing permission cannot
approve it. Retrying an accepted memo never authorizes another debit; renewed preparation must not broaden the approved
bound.

Generic incoming `payment.top_up` needs an active wallet session, not Chat authority or outgoing spend consent. It only
imports supplied coin secrets; it cannot select the Host's existing wallet inventory. Encrypted SSO preserves this
separation at the signing Host.

## Amounts, deposit completion and acknowledgments

Outgoing `SendPayment.amount_cents` is a positive `u64` count of cents of the **configured Coinage asset**. The Host
performs checked conversion and binds denomination metadata into durable operations. It is not a JavaScript floating
point value, a fiat quote, or a chain-native planck count.

Native `CoinageSend.total_value` and generic top-up amounts are **raw `u128` chain units**, not cents. For incoming
Chat:

1. Persist the authenticated raw amount and exact supplied keys in encrypted product recovery storage.
2. Obtain the Host's positive `coinage_cents_unit`; require an exact integral division and checked `u64` result before
   presenting a cent-denominated card. Never hardcode a network/asset conversion or treat raw units as cents.
3. Persist that amount binding, then call `payment.top_up` with `into: None` (main purse), the original raw amount as
   the minimum expected credit, and `PaymentTopUpSource::Coins` containing the same keys on every retry.
4. Mark complete only after the Host reports finalized complete import and the product durably saves that receipt and
   key removal. An underfunded import is not full payment and must not be acknowledged as such.

For the generic API, `amount: 0` imports all supplied coins without a positive minimum. Chat does **not** use that form
for a memo advertising a definite amount. A changed minimum on retry inspects the original receipt; it does not create a
second claim. `PartialPayment { credited }` carries proven raw credited units after a definitively finished import below
the requested minimum. Missing/not-yet-funded or ambiguously spent sources remain unresolved, with custody retained; a
cleared prefix alone must not become a false success or definitive partial result.

The selected wallet service keeps durable import records, canonical source identity, claim plans and finalized receipts.
Reordered keys reuse the same custody; conflicting overlaps and cross-product rebinding are rejected. The Rust backend
uses its encrypted main-purse WAL and preserves original order, denomination binding and destination plans when adopting
its existing incoming memos. The reference iOS backend delegates custody and claim recovery to its native
`IncomingPaymentService`. Neither backend requires a running guest to retain accepted incoming funds.

The product sends a native success ACK only when the **whole original batch** and all HOP pages are durably processed,
and all required top-ups completed. It must not invent per-page request IDs or ACK synthetic batches. Private file
references needed after remote deletion must also be durable. Delayed ACKs preserve native-peer compatibility.

An ACK from a different native implementation may prove custody rather than clearing. Outgoing status therefore keeps
`Delivering`, `Delivered`, `Claiming`, `PartiallyCleared`, `Cleared`, `Recovering` and `Failed` distinct. Only chain
finality proves settlement. A failure must not erase a finalized prefix; late ACKs cannot regress established clearing
evidence.

RFC 0017 instead uses `u32` **dotUSD cents**. Native amounts need checked width and actual asset conversion before that
API can replace them. The previously exercised `paseo-next-v2` profile uses People genesis
`0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48`, instance `0`, and pUSD precision 6: one native
cent is 10,000 chain units. This is configuration, not a default, a new qualification result, or a claim that pUSD is
dotUSD. Hosts must verify the live selected configuration and must not silently substitute another environment.

## Durability, migration and lifecycle

Outgoing identity binds wallet, network, product and caller request ID. Changed recipient or amount returns
`OperationConflict`; a new ID is a new proposal, never an automatic retry. Payment ciphertext acceptance is a durable
handoff, not network submission or peer delivery. The product owns transmission and retries with original native IDs.

The guest stores pending protocol work, incoming claims and legacy archives in encrypted bounded shards. It writes an
authenticated staging/garbage journal before new immutable shards, then atomically replaces the manifest only after all
shards are acknowledged. Only the manifest acknowledgment advances payment or ACK durability gates. Restore verifies
every referenced shard and fails closed on missing/corrupt data. Cleanup deletes only unreferenced scoped keys and
retains its journal across cancellation. Per-call storage limits are unchanged; display retention is separate from exact
recoverable migration archives.

Legacy Host state is frozen until explicit migration commit. Ordinary history, pending ciphertext, invitation native IDs
and caller/native delivery mappings survive this transfer. The Host does not duplicate the entire legacy snapshot just
to freeze it. After commit it retires transferred ordinary state while retaining private device keys, security roster,
payment custody and file state. V1 actor calls and method 11 are not compatibility shims.

The Host runs **no ordinary Chat receiver or outbox service** while the product is closed. It does resume existing
wallet imports and payment recovery after unlock, without a Chat grant, guest initialization, or a new prompt. It must
not activate a competing allocator just because an ordinary Chat product polls. Logout and session replacement fence new
effects; already-owned durable commits may finish. Product revocation/clearing does not erase wallet custody. Suspended
or terminated Hosts still need platform scheduling; this API promises neither OS wake nor push delivery.

HOP is retrieved through trusted configured Bulletin access, not caller endpoints. The Host authenticates bounded nested
history and keeps replayable handoff pages until the product prepares the actual successful native ACK. File selection,
immutable source recovery, private tickets, cached chunks and export remain trusted Host operations. The product
receives public metadata and opaque handles and delivers prepared native rich-content ciphertext. First-contact
attachments and call signaling remain unsupported.

## Main-purse ownership and native service integration

The main purse uses page-0 `//coinage//4294967295//0/<index>` (soft coin item) and
`//coinage-ring-vrf//4294967295//0//<index>` (hard voucher item). Wallet snapshot version 3 rejects legacy `//pps` state
without rewriting or discarding it. Matching derivations alone do not reconcile counters, reservations or pending memos.

The signing Host accepts an optional `CoinageWalletHost` dependency at construction. Without a registered native wallet
it uses the built-in Rust wallet; with one, all wallet operations use that service for the runtime's lifetime. A locked
or failed registered native service never enables Rust custody. There is no selection callback or availability-based
fallback. Products cannot register or replace the runtime's wallet dependency.

The reference `hosts/ios` integration injects `TrUAPINativeCoinage`, an adapter over the coordinator's existing
`CoinageService`, directly into `TrUAPIHostRuntime`. Browser and CLI embeddings omit it and use Rust without extra
configuration. `NativeCoinageRequest` binds the active wallet root, configured Coinage chain and asset instance.
Denomination queries, outgoing preparation/status and generic incoming imports use that service's existing inventory,
counters, balance and recovery. The adapter stores operation/approval bindings, not another inventory or a plaintext
memo file.

Outgoing native preparation retains recipient derivation records and committed custody marks atomically with native
transaction registration (or in one native transaction for an exact-match transfer). This precedes returning bearer
material to trusted Rust code. A restart must not clear these coins as ordinary provisional reservations. Rust seals the
memo for the authenticated recipient, records durable ciphertext custody, then separately commits transport acceptance.
That acceptance is neither peer delivery nor chain finality. Repeated operation IDs replay retained preparation;
conflicting intent is rejected. Native trusted review includes exact debit and any required privacy warning.

The native `CoreStorageKey::MainPurseCoinage` guard (index 13) remains. Native dispatch does not read that slot or
create the Rust allocator. Matching derivations is not permission to run both owners or migrate an existing purse. A
payment batch without available safe custody remains unacknowledged; ordinary Chat is not presented as a successful
payment fallback. Native activation generations fence suspended work even when logout is followed by login to the same
root.

Native incoming receipts and retained sources must bind a stable wallet-root/chain/instance owner, so same-owner restart
can resume claims but another wallet cannot adopt them. Pre-upgrade incoming rows without a provable owner remain
preserved, including their source secrets, but are not automatically resumed under the current root. Such pending
imports need explicit ownership recovery; this integration does not infer ownership or erase them during the additive
Core Data schema upgrade.

Method 12 exposes no balance. Summing Chat cards cannot produce spendable wallet balance; other products, reservations
and channels are omitted. `payment.balance_subscribe` is not implemented as working Chat balance support. Existing
trusted wallet balance visibility must be preserved by actual reconciled inventory, not a synthetic guest total.

Local codec/custody tests, callback smoke execution and generated Swift bindings are not native iOS compilation, funded
native-peer interoperability or cross-platform qualification. Rebuild the local iOS XCFramework together with its
bindings before compiling the reference app; freshly generated bindings cannot be paired with an older binary. Native
qualification still requires denied review, stale sessions, dropped calls, interrupted writes, restart, roster changes,
ambiguous/partial claims, files and funded native-peer checks on the consuming Host. Prior funded round trips through
the former actor do not prove this boundary.

## Planned RFC 0017 transition

- Create a receivable for `MAIN_PURSE`; its private key stays in the receiving Host.
- Authorize a main-purse debit in the sending Host and create a cheque encrypted to the receivable. The product carries
  the opaque cheque, never outgoing coin secrets.
- Deposit into the associated main purse and report actual clearing progress; no separate Chat purse or follow-up sweep
  is required.
- Retain product-owned ordinary Chat and transport. Generic `top_up(Coins)` is the interim native import mechanism, not
  RFC 0017 `deposit`.

Cutover requires an agreed interoperable cheque encoding, peer support, authenticated recipient/receivable binding and
checked asset/amount conversion. A caller-supplied receivable is not proof of a displayed contact identity. Pending
native memos, reservations, claim plans and operation IDs must survive until settled or safely recovered. Do not
reinterpret native messages as cheques, issue replacement payments silently, or add a second allocator.
