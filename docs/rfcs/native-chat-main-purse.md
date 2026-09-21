---
title: "Host-owned native Chat and main-purse payments"
owner: "@replghost"
status: draft
---

# RFC — Host-owned native Chat and main-purse payments

## Summary

This draft specifies the Host-owned native Chat actor on TrUAPI Account method
12, including one-shot, explicitly reviewed Coinage payments from the user's
main purse. Products receive authenticated conversation views and payment
status, while the signing Host retains device secrets, payment memos, durable
claim plans, and settlement authority. It builds on the custody and main-purse
model of [RFC 0017](0017-coinage-payment.md), without claiming implementation of
that RFC's complete CoinPayment API.

This proposal is **draft, not approved**. It accompanies the implementation in
[host-rust-core #709](https://github.com/paritytech/host-rust-core/pull/709) for
review; inclusion here is neither release availability nor deployment evidence.

## Motivation

Guest-held Chat cryptography lets a product handle identity ciphertext and
spendable payment material that should belong exclusively to the wallet. A
product reload can interrupt receipt between decryption and durable custody;
an acknowledgment can then destroy the sender's recovery path before the
recipient has a recoverable claim. A generic Chat permission also cannot
safely authorize debits from the user's ordinary balance.

A Host-owned actor gives native and browser products the same restricted
interface. It authenticates the peer roster, retains recoverable operations
independently of a guest call, and separates delivery from finalized clearing.
The product can ask to send a payment, but cannot choose coin inputs, construct
proofs, approve itself, or obtain the secrets needed to spend received funds.

## Approach

### Scope and relationship to RFC 0017

RFC 0017 defines `MAIN_PURSE = u32::MAX`, the user's ordinary Coinage purse,
its secret-custody boundary, and general purse/receivable/cheque APIs. This RFC
uses that main-purse concept for a particular authenticated native Chat channel.
Incoming Chat payments are claimed into the main purse; outgoing payments debit
it. Conversation identity and product permissions do not create independent
purses or product-owned balances.

Method 12 does **not** expose `create_purse`, `query_purse`,
`rebalance_purse`, `delete_purse`, `create_receivable`, `create_cheque`,
`listen_for_payment`, `deposit`, or `refund`. A native Chat payment memo is not
a product-visible RFC 0017 cheque. Supporting this RFC MUST NOT be advertised as
complete RFC 0017 support, generic merchant checkout, invoice support, automatic
refunds, or a reusable spending allowance. RFC 0017's proposed purse selectors
on RFC 0006 are not added by this method.

The normative requirements below describe the proposed contract. The final
section separately identifies implementation evidence and integration limits;
requirements are not assertions that every embedding Host already meets them.

### Wire contract and compatibility

The source of truth is:

- [`api/account.rs`](../../rust/crates/truapi/src/api/account.rs):
  `Account::product_device_chat`, trait ID **2**, method ID **12**;
- [`versioned/account.rs`](../../rust/crates/truapi/src/versioned/account.rs):
  `HostProductDeviceChatRequest::V1`, response `::V1`, and domain error `::V1`;
- [`v02/account.rs`](../../rust/crates/truapi/src/v02/account.rs): the V1 payload
  definitions. The `v02` source module is not a `V2` wire envelope.

The method is a request/response call returning
`Result<HostProductDeviceChatResponse, CallError<HostProductDeviceChatError>>`.
It is not a guest subscription. The Host separately owns receive subscriptions.
Generated bindings MUST preserve the canonical enum encodings, integer widths,
and version envelope; a package version alone is not capability negotiation.

Account method **11** is permanently retired. Its former raw Open/Seal/proof
interface MUST NOT be forwarded locally or over SSO, reassigned, or emulated by
method 12. Unsupported methods, envelope versions, and unavailable Host
implementations MUST fail through the protocol/`CallError` mechanism rather
than appear successful or leave requests pending. The default method 12 trait
implementation returns `CallError::unavailable()`; the domain error enum has no
`Unsupported` variant. Hosts MUST NOT translate unsupported operations into
empty successful Chat views, fabricated payments, or raw-crypto fallback.

A method-11 guest and a method-12 Host are intentionally incompatible. Guest,
SDK, generated codecs, core, and platform adapters MUST be upgraded together.
An older release carrying the same broad core version is not sufficient unless
its actual source/artifact contains this contract.

### Public requests

The V1 request enum contains exactly these operations. `Id32` in this table
means the canonical `[u8; 32]`, not an additional wire type.

| Variant | Fields | Contract |
| --- | --- | --- |
| `Initialize` | none | Open or restore the Host-owned device and return public state. It does not authorize spending. |
| `Invite` | `username: String`, `text: String` | Resolve the identity and Chat key on the configured network and create a native invitation with ordinary welcome text. |
| `Receive` | `statement: SignedStatement` | Authenticate the complete signed native statement, decrypt privately, and commit custody-sensitive effects before acknowledgment. Passing bytes is not an authentication assertion. |
| `AcceptInvitation` | `invitation_id: Id32` | Accept an invitation already authenticated and retained by the Host. |
| `RejectInvitation` | `invitation_id: Id32` | Reject a retained invitation; this is not a payment cancellation operation. |
| `Send` | `peer_identity: Id32`, `request_id: String`, `messages: Vec<Vec<u8>>` | Send validated ordinary native message encodings to the established roster. Payment, arbitrary ciphertext, and device-control injection are forbidden. |
| `SendPayment` | `peer_identity: Id32`, `request_id: String`, `amount_cents: u64` | Propose one main-purse payment to the Host-authenticated recipient, subject to trusted per-spend review. |
| `PaymentStatus` | `operation_id: Id32` | Read this product's durable operation status without proposing a new debit. |
| `Reconcile` | none | Resume authorized durable transport/recovery work and return public views; it grants no new spending permission. |
| `SendAttachments` | `peer_identity: Id32`, `request_id: String`, `text: Option<String>` | Select immutable files in trusted Host UI and send safe rich content. The guest cannot pass a local path or upload credential. |
| `OpenAttachment` | `attachment_id: Id32` | Resume private download and present/export through trusted Host UI, without returning file bytes to the guest. |

Caller request IDs MUST be nonempty, at most 128 UTF-8 bytes, and contain no
control characters. Welcome text is bounded to 8192 UTF-8 bytes. Native message
and attachment codecs impose further structural and resource bounds; invalid
or forbidden payloads return `InvalidRequest`, not a permissive opaque tunnel.
Hosts MUST bound queues, histories, storage, and nested history expansion, and
fail closed when they cannot retain recovery records. Capacity exhaustion MUST
NOT silently evict live payment custody or idempotency commitments.

### Public responses and errors

Every successful operation returns the same V1 response:

| Field | Public content |
| --- | --- |
| `device: HostNativeChatDevice` | Wallet identity account/public Chat key, product allowance account, and Host-owned device statement account/public Chat key. |
| `peers: Vec<HostNativeChatPeer>` | Authenticated identity, optional resolved username, admitted device accounts/public keys, incoming subscription topics, and `ready_for_payments`. The guest cannot write this roster back. |
| `invitations: Vec<HostNativeChatInvitation>` | Stable invitation ID, authenticated peer identity, optional resolved username, native millisecond timestamp, and ordinary welcome text. |
| `messages: Vec<HostNativeChatMessages>` | Peer, incoming/outgoing direction, native request ID, and validated ordinary message encodings with custody-sensitive content removed. |
| `acknowledgments: Vec<HostNativeChatAcknowledgment>` | Peer, native request ID, and native `response_code: u8`; zero means successful delivery processing, not clearing. |
| `payments: Vec<HostNativeChatPayment>` | Product-scoped payment cards described below. |
| `rich_messages: Vec<HostNativeChatRichMessage>` | Authenticated message/reply/edit metadata, text, opaque attachment IDs, safe media metadata, and transfer progress. |

Public views may repeat retained messages and acknowledgments. Consumers MUST
correlate native request/message IDs and upsert payment cards by `operation_id`;
response arrival is not an exactly-once event or a complete historical archive.
`PaymentStatus` uses the common response, not a separate one-card schema.

A payment card has `operation_id: [u8; 32]`, `request_id: String`,
`message_id: String`, `timestamp: u64` (milliseconds),
`peer_identity: [u8; 32]`, `direction: Incoming | Outgoing`,
`amount_cents: u64`, and `state: HostNativeChatPaymentState`.
It contains no memo, source coin identifiers, private clearing evidence, asset
selector, or wallet balance. Products MUST obtain asset/network presentation
from trusted Host context, not infer a currency from the integer alone.

Attachment metadata contains `mime_type`, `size_bytes: u32`, and kind `File`,
`Image { width, height, thumbnail }`, or
`Video { duration_seconds, thumbnail }`. Thumbnails are optional validated
BlurHash bytes, not URLs or executable images. Attachment progress is
`Preparing`, `Uploading { uploaded_bytes }`,
`Downloading { downloaded_bytes }`, `Ready`, or `Recovering`.
These states are independent of both message acknowledgment and payment status.

Domain errors are the exact `HostProductDeviceChatError` variants:

| Error | Meaning and caller action |
| --- | --- |
| `NotConnected` | No current authenticated wallet session; restore a session before retry. |
| `AccessNotGranted` | Required Chat/transport capability is absent or revoked; do not bypass it. |
| `UserRejected` | Trusted review, file selection, or export was declined; do not loop prompts automatically. |
| `AllowanceRequired` | The Host device needs its Statement Store allowance. |
| `PeerNotReady` | The peer has not met the applicable establishment/device eligibility requirements. |
| `OperationConflict` | Immutable parameters or an existing asset binding conflict; changing an ID is not a safe automatic retry. |
| `InvalidRequest` | Invalid bounds, amount, identifier, or unsupported native message content. |
| `InvalidStatement` | Authentication, decryption, or authenticated native content validation failed. |
| `RecipientNotFound` | Identity resolution failed on the selected network. |
| `InsufficientBalance` | The main purse cannot fund the proposed payment. |
| `StorageUnavailable` | A safe durable commit is unavailable; never assume the operation had no prior effects. |
| `NetworkUnavailable` | Configured chain/transport is unavailable; reconcile before deciding an operation failed. |
| `OperationNotFound` | No matching payment visible to this product; do not reveal another product's operation. |
| `AttachmentsUnavailable` | This Host cannot select, recover, or present the requested private file. |

Errors MUST be sanitized. Neither domain errors nor transport diagnostics may
contain secrets, decrypted payment memos, file credentials, or raw backend
errors that carry them. Call failure or timeout is not payment cancellation.

### Identity, product, network, and device eligibility

The authenticated calling product comes from Host connection/SSO context,
never request-supplied labels. Chat state is scoped to the wallet, selected
network, product, and Host installation's device. The main-purse allocator is
wallet/network owned and serialized across products. Product operation IDs and
attachment handles MUST NOT provide access across wallets, networks, or products.

The Host resolves usernames and root Chat public keys using trusted network
configuration and authenticates native invitations and device-control messages.
Guests select only an established peer identity, not encryption keys, device
lists, signing accounts, network endpoints, or Coinage asset instances. SSO
MUST preserve this boundary at the signing Host; the execution Host cannot
substitute its own review for the signing Host's authorization.

Ordinary Chat requires an established peer with active authenticated devices.
Outgoing payment eligibility additionally requires at least one active, keyed
peer device to acknowledge the legacy-device revocation update. The payment
envelope includes only the eligible acknowledged devices. An offline advertised
device need not block an eligible recipient. A payment ACK MUST originate from
a recipient included in the committed envelope. Authenticated roster changes
invalidate eligibility until it is re-established; rewrapping a retry MUST
preserve the payment identity and exact memo, not create a second spend.

Incoming payments require an authenticated admitted sender and durable custody;
they do not apply the outgoing `ready_for_payments` gate to the sender. First
contact is not a channel for embedding payments or attachments in welcome text.
Call signaling remains unsupported. Native push-token metadata may be validated
and discarded privately, but this does not imply a push provider or OS wake.

### Permissions and trusted per-spend consent

Chat authority is a dedicated authorization, separate from disclosure of a
username and separate from `StatementSubmit`. The Host-owned receiver requires
current Chat authority and Statement Store submission permission. Attachment
uploads additionally require the existing Bulletin allowance and
`PreimageSubmit`; none of these grants permits spending.

Each new outgoing debit MUST obtain trusted signing-Host review of
`MainPurseChatPaymentReview`: authenticated calling product, recipient identity,
Host-resolved username when available, exact `amount_cents`,
`max_debit_cents` including the approved fee bound, chain `genesis_hash`,
trusted `coinage_instance_id`, and immutable `operation_id`.
The UI MUST make the selected network, asset, recipient amount, and maximum
main-purse debit unambiguous. It MUST NOT render a guest label as authenticated
recipient identity or approve through guest JavaScript, auto-sign policy,
Chat authorization, or generic product signing permission.

Approval authorizes only that immutable operation and debit bound. Transport
replay of an already accepted memo does not authorize another debit. Resuming
unfinished preparation may require renewed review of the same operation; the
Host MUST NOT silently broaden the prior approval. Denial before effects can
cancel the intent. Denial after an accepted effect prevents new effects but
cannot erase an existing transfer, release ambiguously spent inputs, or promise
a refund. `PaymentStatus` is not a consent prompt or cancellation API.

### Integer amounts and asset mapping

Chat V1 `amount_cents` is an unsigned 64-bit integer. An outgoing amount MUST be
in `1..=u64::MAX` and also pass the selected runtime's denomination, checked
conversion, inventory, and maximum-debit bounds. Zero, overflow, unsupported
units, and a non-integral exact received amount MUST be rejected. Products and
bindings MUST NOT pass monetary values through an inexact floating-point or
JavaScript `Number` conversion for values above its safe integer range.

The unit is one cent of the **Host-selected Coinage asset**, not a chain-native
planck and not an arbitrary fiat quote. The Host obtains denomination metadata
from the selected chain/instance and uses checked arithmetic to convert cents
to `u128` chain units. Exact amounts require integral conversion; partial
clearing reports only proven whole cents (rounding down), while a review's
maximum debit rounds up if necessary so it never understates the bound.
The durable operation binds denomination metadata; a changed runtime mapping
MUST NOT reinterpret an in-flight payment.

RFC 0017 instead specifies `Balance = u32` in **dotUSD cents**, exponent 2.
These types are not interchangeable aliases. An adapter to that API MUST check
`amount_cents <= u32::MAX` and the actual dotUSD asset/denomination before a
lossless conversion; truncation and relabeling are forbidden. The qualified
local native profile is `paseo-next-v2`, People genesis
`0x4a2b5b737de1da59e209b0000a876ec2fa20035dc34fd292a848da32d255ad48`,
Coinage instance `0`, with pUSD chain-asset precision 6. In that profile, `1`
cent is `0.01 pUSD` (10,000 chain units). These are explicit configuration
values, not values inferred from `MAIN_PURSE`. Hosts MUST verify the live
configured chain and denomination metadata rather than substitute another
Paseo genesis or assume any instance is equivalent. This mapping is not a
claim that pUSD is dotUSD or redeemable fiat USD.

The chain genesis and optional Coinage instance are Host configuration, not
request fields. `None` is valid only for a legacy single-asset runtime; an
instance-scoped runtime requires its trusted instance ID. An opened encrypted
wallet binds its selected instance permanently. Configuration changes MUST fail
closed instead of retargeting reservations, claims, or pending payments.
A product unable to identify the configured asset MUST NOT invent a currency
label or offer an ambiguously denominated payment.

### Durable custody, acknowledgment, and clearing

The Host MUST authenticate and decrypt native traffic privately, validate the
whole batch, and durably retain every payment memo and its complete claim plan
before issuing the corresponding native ACK. This applies to inline batches,
HOP history, and nested compacted history. Private attachment references needed
for later download must likewise be durable before acknowledgment. A storage
failure leaves the message unacknowledged and retryable; partial processing
cannot justify acknowledging the entire batch.

The sender MUST retain immutable memo custody and an outbox commitment before
transmission. The receiver MUST retain enough information to resume claims after
restart and after remote history or pool data is deleted. A claim submission or
an RPC success is not proof of ownership at finality. Chain evidence, rather
than guest assertions, delivery responses, or synthetic balances, determines
settlement.

| Payment state | Meaning |
| --- | --- |
| `Preparing` | Approved inputs are reserved and required preparation is in progress. |
| `Delivering` | Durable encrypted memo exists; transport is being attempted or retried. |
| `Delivered` | An eligible peer acknowledged custody/processing; clearing is not yet established. |
| `Claiming` | Incoming secrets and claim plans are durable; on-chain claiming is in progress. |
| `PartiallyCleared { cleared_cents }` | Finalized evidence covers only part of the amount; do not label the whole payment paid. |
| `Cleared` | The complete payment has been verified at chain finality. |
| `Recovering` | Effects are ambiguous and require reconciliation; reservations and recovery records remain. |
| `Failed { reason }` | Definitive failure after possible prior effects have been reconciled. |

Failure reasons are `Cancelled`, `InsufficientBalance`, `AlreadySpent`,
`InvalidMemo`, and `ChainRejected`. A terminal failure MUST NOT obscure any
value that actually cleared. The V1 `Failed` variant carries no cleared amount;
a Host that cannot represent a mixed outcome truthfully MUST retain the
partial/recovery status rather than collapse it into a misleading failure.

“Claimed” is not a separate V1 enum variant. UI copy MUST distinguish a claim
attempt (`Claiming`) from finalized ownership (`Cleared`). State observations
may skip intermediate states; implementations MUST preserve established
clearing evidence rather than regress it when an ACK arrives late.

### Idempotency, retries, and lifecycle

Outgoing payment identity binds wallet, network, calling product, and caller
request ID. Retrying `SendPayment` with that ID MUST keep the recipient and
amount unchanged or return `OperationConflict`. It resumes the same durable
operation, reservations, and memo. A new ID is a new payment proposal, never an
automatic recovery tactic. Ordinary `Send` deduplication additionally scopes the
request ID to the peer and commits the message content; attachment intents
retain the original recipient, caption, and immutable selected source.

`Invite` does not accept a caller idempotency key and creates a fresh
invitation; a product MUST reconcile before blindly retrying it after a lost
response. Replayed authenticated incoming requests MUST repair pending ACK
transmission without duplicating custody or replacing original commitments.
Responses use the responder's own outgoing identity/device route, not a copy
of the requester's session direction.

Dropping a guest call or closing the product does not cancel an already owned
durable operation. While the Host process is running, its authorized receiver
owns subscriptions, reconnects, and reconciliation independently of the guest.
It MUST recheck session and permissions before new effects and stop on
revocation, logout, or session replacement, including during a stalled network
call. Already handed-off durable commits may complete, but stale sessions MUST
NOT create successor work. Revocation does not delete pending custody or undo a
finalized payment.

Restart restores durable device/payment state, not a guest-held secret cache.
The Host MUST persist a bounded wallet/network index of initialized products
and, after wallet unlock, restore receivers only for entries whose Chat
authority and Statement Store submission permission remain authorized. This
restoration MUST NOT require an open guest or prompt for new authorization.
The index is Host-private wallet state, not product-owned storage. Clearing one
product may durably unregister that product and stop its receiver, but MUST
preserve other registrations and pending payment custody/history. Its storage
adapter must preserve the canonical `CoreStorageKey::NativeChatProducts` entry
(index 16), alongside the actor and wallet stores, and reject malformed or
over-limit data without fabricating an empty initialized-product set. Restoring
a registration MUST NOT regenerate a missing device key.

OS suspension or termination still stops in-process progress. Background wake,
push delivery, and platform scheduling are embedding-Host responsibilities,
not promises of this API. Hosts MUST expose unavailable or pending service state
honestly rather than label suspended work completed.

### Secret custody, migration, and balance visibility

The signing Host owns device private keys, encrypted roster/outbox, payment WAL,
coin inventory, reservations, claim/recovery plans, spendable memos, recycler
and voucher material, and proofs. None may enter guest storage, public errors,
logs, attachment handles, or response payloads. Private file tickets, URLs,
source handles, and file bytes remain Host-owned. HOP uses trusted Bulletin
endpoint configuration, never a guest-supplied network destination.

Migration is a clean cutover: upgrade the guest to method 12, initialize a
Host-owned device, establish authenticated peer rosters, and complete the
legacy-device revocation handshake before sending payments. An old guest
roster or ciphertext is not authority to import identity keys or spending
material through `Send` or `Receive`. Old pending guest-held payments require a
trusted recovery procedure; method 12 provides no guest secret-import API.
Hosts MUST NOT silently delete unresolved old funds or fall back to method 11.

The current main-purse key profile uses page 0:
`//coinage//4294967295//0/<index>` (soft item) and
`//coinage-ring-vrf//4294967295//0//<index>` (hard item). Snapshot version 3
rejects legacy `//pps` snapshots without modifying them. Counters, reservations,
and pending memos MUST NOT be reinterpreted under new derivations. Matching
paths are not sufficient to share an allocator: native iOS CoreData/Keychain
state and the Rust Host snapshot need explicit reconciliation and one allocator
owner before both access the same wallet. A Host MUST NOT enable the Rust
main-purse spend path while an independent native allocator can spend the same
inventory. This is a release blocker until reconciliation and exclusive
ownership are enforced, even when both allocators derive the same keys.

Method 12 deliberately has no balance query. A product MUST NOT calculate a
spendable main-purse balance by summing conversation cards: cards omit other
products, other payment channels, reservations, and wallet history. Trusted
wallet UI may show a balance backed by its actual reconciled inventory. The
current runtime's `payment.balance_subscribe` rejects with `PermissionDenied`;
this draft does not present it as working Chat balance support. Product-visible
balance or RFC 0017 `query_purse` requires its own implemented, authorized
contract before a product may rely on it. Missing balance access is not zero
balance, and wallet migration MUST preserve existing trusted balance visibility
without inventing a guest balance.

## Implementation evidence and remaining integration boundaries

The referenced source implements the actor, typed views, durable stores,
Coinage engine integration, trusted review callback, and in-process receive
service. The implementation effort reports a real local native Host-to-iOS and
iOS-to-Host `0.01 pUSD` payment clearing in each direction. That narrowly scoped
observation is not evidence of general RFC 0017 compliance, Android/desktop or
browser parity, OS background delivery, or a published product release. This
document introduces no additional test or deployment result.

The reference `hosts/ios` integration still has a separate native
`CoinageService`. It rejects access to `CoreStorageKey::MainPurseCoinage`
(index 13), preventing the Rust actor from scanning, allocating, or claiming
that inventory. Ordinary Chat remains available, but main-purse Chat payment
custody is unavailable and incoming payment batches must not be acknowledged.
This fail-closed guard is not a native-store migration. An embedding Host with
one shared Rust wallet owner may enable payments only when its actual durable
storage, review UI, and lifecycle satisfy the contract above.

The following remain release/integration obligations rather than implied
capabilities:

- Each embedding Host must implement the trusted spend review and durable
  storage boundary, and consume artifacts built from the matching actor and
  native codec source. Merely selecting a core version is insufficient.
- Same-wallet native/Rust allocator migration is a release blocker wherever
  competing writers can spend the same inventory. Explicit custody/counter
  reconciliation and one allocator owner are required. Separate devices
  exchanging payments do not prove safe concurrent allocator sharing.
- Guests need trusted asset/network display context because V1 payment cards do
  not carry it. A Host unable to establish the mapping must disable the spend
  path, not substitute a familiar currency name.
- Product balance APIs, general RFC 0017 purse/receivable/cheque operations,
  and OS wake/push are not supplied here.
- Indexed post-unlock receiver restoration is implemented in the core source,
  but must still be qualified with durable permission restoration and platform
  storage. The existing in-process receive service and unexecuted regression
  cases alone are not evidence of cold-restart conformance.
- Qualification must separately exercise denial, revoked sessions, dropped
  calls, interrupted durable writes, restart/replay, roster changes, partial
  claims, and ambiguous chain outcomes on the actual consuming Hosts. A funded
  happy-path round trip does not establish these guarantees by itself.

## Trade-offs

- A Host-owned actor reduces guest flexibility and makes platform adapters and
  trusted UI mandatory, but prevents a compromised product from retaining
  spendable Chat material or approving its own payment.
- Retiring method 11 requires a coordinated upgrade rather than a transparent
  compatibility shim. Keeping the raw interface would preserve the custody
  violation this proposal is intended to remove.
- Durable custody before ACK consumes storage and can delay transport progress.
  Acknowledging earlier trades that cost for unrecoverable funds and is rejected.
- `u64` native Chat amounts preserve the existing actor contract but require
  explicit checked adaptation to RFC 0017's narrower dotUSD `Balance`.
- Sharing the main purse gives users their ordinary wallet funds, but requires
  wallet-wide serialization and migration instead of independent per-product
  coin allocators. Separate product purses remain the concern of RFC 0017.
- Delivery and settlement are deliberately separate, so a product must show
  pending/recovery states. Optimistic “paid” status based on an ACK is rejected.
