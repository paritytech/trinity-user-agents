---
title: "NFT Purses"
owner: "@valentunn"
status: draft
---

# RFC — NFT Purses

|                 |                                                                                                   |
| --------------- | ------------------------------------------------------------------------------------------------- |
| **Start Date**  | 2026-10-02                                                                                        |
| **Description** | A purse API for NFT items with per-item keys, idempotent chain operations, and backup recovery.  |
| **Authors**     | Valentin Sergeev                                                                                  |

## Summary

This RFC defines a host API for keeping NFT items in firewalled purses. It is based on
[RFC 0017](0017-coinage-payment.md) and changes it in four ways:

- querying and rebalancing a purse return individual item public keys;
- a cheque carries its item public keys in a cleartext part;
- every chain-exposed operation is registered under an idempotency key and observed by that key, so a product can
  recover the outcome after a restart;
- the host's obligations when restoring purses from a backup are defined.

## Motivation

RFC 0017 exposes purses as balances. Products working with NFT items need to address individual items: list them,
move a specific set of them between purses, hand a specific set over to another user, and transfer one of them to an
external key.

Purse operations settle on chain and can outlive the product session that started them. A product that reloads, or a
host that restarts, must be able to learn what happened to an operation instead of starting a second one over the same
items.

The host must also prevent the same item from being spent twice, for example by a rebalance while a cheque for that
item is in flight. This requires the host to know which items are busy, which in turn rules out letting products
build arbitrary transactions over purse items.

Finally, a reinstalled host restores purses from a backup. Products need a way to rediscover their purses, and the
RFC must state what the host is and is not obliged to recover.

## Explanation

### Design decisions

- Purse queries are paginated, since one purse can hold hundreds of items.
- `list_purses` lets a product rediscover its purses after a host reinstall.
- Chain-exposed operations are asynchronous. A call only registers the operation under an idempotency key;
  `observe_operation_status` follows it. Registering a key twice fails.
- The host tracks which items are in use, either by an active operation or because they were handed off in a cheque,
  and refuses to consume them again. `revoke_cheque` releases items handed off in a cheque.
- Products cannot sign arbitrary transactions with purse items, because the host could then not track item status.
  `transfer_item` covers the one external transfer products need.
- Backup recovery restores assets only. In-use statuses and purse-to-product ownership are not guaranteed to survive,
  see [Backup recovery](#backup-recovery).

### API placement

The calls below form a separate `NftPurse` trait beside RFC 0017's `CoinPayment` trait, which is unchanged. Method
names match their `CoinPayment` counterparts where the operation is the same. As with `CoinPayment`, the
implementation prefixes the trait's types with the trait name (`NftPurseCheque`, `NftPurseStatus`, ...); this RFC
leaves the prefix out for readability.

### Purse ids and ownership

```rust
type PurseId = u32;
const MAIN_PURSE: PurseId = u32::MAX;

enum PurseOwner {
    // Purse is owned by the product
    Product(ProductId),
    // Purse is owned by the root: no product can read it without a grant or drain it without a confirmation.
    // The main purse is root-owned, and so are purses recovered without metadata
    // (see "Backup recovery").
    Root,
}
```

A purse created with `create_purse` is owned by the calling product, which uses its own purses without any check. A
purse owned by `Root` or by another product is checked in one of two ways:

- Reading: `list_purses`, `query_purse` and `create_items` require the `NftPurseRead` user-agent permission, granted
  per owner. One grant covers every purse of that owner.
- Draining: calls that move items out of the purse (`rebalance_purse`, `create_cheque`, `transfer_item` and `refund`)
  ask the user for a confirmation on every call, like `createTransaction`. Nothing is stored, so a confirmation never
  covers a later call.

```rust
enum UserAgentPermission {
    // existing variants...
    NftPurseRead { owner: PurseOwner },
}
```

A `CoinPayment` grant does not cover NFT purses, and an `NftPurseRead` grant does not cover coin purses. A grant for
`Root` does not cover any product, and a grant for one product does not cover another.

### Purse items

```rust
type ItemKey = [u8; 32];

// Logical status of an item. Not exposed to products directly; defined so that the
// calls below can refer to it. Whether the host stores it or derives it from the
// transaction graph is an implementation detail.
// Only Idle items can be consumed by an operation.
enum ItemStatus {
    // Available for operations
    Idle,
    // In use by an active operation, or handed off in a cheque.
    // Reverts to Idle if the operation fails terminally or the cheque is revoked.
    InUse,
}
```

### Purse management

#### Create purse

```rust
// `name` is at most 64 bytes of UTF-8. A longer name fails with GenericError.
fn create_purse(name: String) -> Result<PurseId, GenericError>;
```

#### Create items in purse

```rust
// Create `count` new keys in `purse`.
// The returned vec has exactly `count` entries.
// `count` must be in [1, 1000].
// Requires NftPurseRead { owner: owner(purse) } if owner(purse) != PurseOwner::Product(self).
fn create_items(purse: PurseId, count: u32) -> Result<Vec<ItemKey>, CreateItemsError>;

enum CreateItemsError {
    Denied,
    CountOutOfRange,
    Other(GenericError),
}
```

#### Query purse

```rust
// Return the items that are present on chain AND are Idle, i.e. the spendable items.
// A page past the end returns { has_next: false, items: [] }.
// Requires NftPurseRead { owner: owner(purse) } if owner(purse) != PurseOwner::Product(self).
fn query_purse(
    purse: PurseId,
    page: u32,
    page_size: u32,
) -> Result<PurseInfoPage, QueryPurseError>;

struct PurseInfoPage {
    has_next: bool,
    items: Vec<ItemKey>,
}

const MAX_PAGE_SIZE: u32 = 100;

enum QueryPurseError {
    Denied,
    // page_size < 1 or page_size > MAX_PAGE_SIZE
    PageSizeOutOfBounds,
    Other(GenericError),
}
```

#### List purses

```rust
// List all purses owned by `owner`.
// A product without purses gets an empty vec.
// Requires NftPurseRead { owner } if owner != PurseOwner::Product(self).
fn list_purses(owner: PurseOwner) -> Result<Vec<Purse>, ListPursesError>;

struct Purse {
    id: PurseId,
    // After backup recovery this may be chosen by the host,
    // if the host could not recover purse metadata.
    name: String,
}

enum ListPursesError {
    Denied,
    Other(GenericError),
}
```

#### Rebalance purse

```rust
// Consume `keys` from `from` and mint new keys in `to`.
// Returns once the operation is registered; follow it with `observe_operation_status(key)`.
// The items are InUse while the operation is active.
// Asks for a confirmation if owner(from) != PurseOwner::Product(self).
fn rebalance_purse(
    from: PurseId,
    to: PurseId,
    keys: Vec<ItemKey>,
    key: IdempotencyKey,
) -> Result<(), RebalancePurseError>;

enum RebalancePurseError {
    Denied,
    // `from` or `to` was not found
    PurseNotFound,
    // `keys` is empty
    NoItems,
    // Some item in `keys` was not found in `from`
    ItemNotFound,
    // Some item in `keys` is not Idle
    ItemUnavailable,
    // An operation with `key` is already registered
    OperationExists,
    Other(GenericError),
}
```

#### Operation status

`rebalance_purse`, `deposit`, `refund` and `transfer_item` register an operation under an idempotency key, and
`observe_operation_status` follows any of them.

```rust
// Chosen by the product, scoped to it: another product's key is never visible.
// A product can derive it from the call's inputs, for example
// blake2_256(SCALE((from, to, sorted keys))) for a rebalance.
type IdempotencyKey = [u8; 32];

// Emits the status of the operation registered under `key`, and ends after a terminal value.
// An operation stays observable after it ends.
fn observe_operation_status(
    key: IdempotencyKey,
) -> Result<Subscription<Status>, ObserveOperationStatusError>;

enum ObserveOperationStatusError {
    // No operation of this product is registered under `key`
    OperationNotFound,
    Other(GenericError),
}

enum Status {
    // Emitted while neither an optimistic success nor a terminal failure is observed.
    InProgress,

    // Per-item clearing progress.
    // Emitted while at least one item is not yet cleared in a finalized block.
    Clearing {
        cleared: Vec<ClearedItem>,
    },

    // Terminal. All items are cleared in a finalized block;
    // every ClearedItem.finalized is true.
    Cleared {
        cleared: Vec<ClearedItem>,
        reference: ClearingReference,
    },

    // Terminal. All transactions are settled and some of them failed.
    Incomplete {
        cleared: Vec<ClearedItem>,
        // Source keys of the failed items
        failed: Vec<ItemKey>,
        reference: ClearingReference,
    },
}

struct ClearedItem {
    // Source item, from the origin purse or from the cheque
    consumed: ItemKey,
    // Destination item, in the destination purse
    minted: ItemKey,
    // Whether the transfer transaction is finalized
    finalized: bool,
}

struct ClearingReference {
    // Binary Merkle root of the SCALE-encoded `leaves`, as `binary_merkle_tree::merkle_root`
    // computes it with Blake2-256. A verifier must check the leaf count.
    root: [u8; 32],
    // Source item and the transaction that moved it, sorted by item
    leaves: Vec<(ItemKey, TransactionHash)>,
}

type TransactionHash = [u8; 32];
```

### Handing off items

```rust
type ReceiveKey = [u8; 32];
// As defined in RFC 0022
type X25519PublicKey = [u8; 32];
// Opaque cheque id
type ChequeId = [u8; 32];

enum ChequeDestination {
    // One-time ReceiveKey created by the receiver with `create_receive_key`, for
    // transmission over a channel dedicated to this transfer. The ReceiveKey is the
    // transfer identity and idempotency key; the host derives the ChequeId from it.
    // The payment is receiver-initiated: the receiver has to hand the ReceiveKey to
    // the sender first.
    Ephemeral(ReceiveKey),

    // Long-lived destination managed by a receiver that expects cheques encrypted
    // to `encryption_key`. The payment is sender-initiated: the receiver publishes
    // a static key and a transmission channel and monitors the channel for cheques.
    Static {
        // Recipient key for x25519 key agreement
        encryption_key: X25519PublicKey,
        // Identity and idempotency key
        cheque_id: ChequeId,
    },
}

// The part of ChequeDestination the recipient needs to find the decryption key.
enum ChequeRecipient {
    Ephemeral(ReceiveKey),
    Static,
}

struct Cheque {
    version: u8, // 0
    id: ChequeId,
    recipient: ChequeRecipient,
    items: Vec<ItemKey>,
    encrypted_secrets: Vec<u8>,
}
```

On the sender side, the host associates a cheque with the product that created it with `create_cheque`. On the
receiver side, it associates a cheque with the product that deposited it with `deposit`. These associations, not purse
ownership, decide who may call `revoke_cheque` and `refund`.

#### Create one-time receive key

```rust
// Receiver side: start a ChequeDestination::Ephemeral handoff into `into`.
// Needs no grant or confirmation: receiving only adds items to a purse.
fn create_receive_key(into: PurseId) -> Result<ReceiveKey, CreateReceiveKeyError>;

enum CreateReceiveKeyError {
    PurseNotFound,
    Other(GenericError),
}
```

#### Listen for payment

```rust
// Receiver side: select and monitor the transmission channel for a
// ChequeDestination::Ephemeral payment.
// First emits PaymentListenForItem::Channel with the selected channel, then
// PaymentListenForItem::Cheque once the cheque arrives.
// The receiver hands the channel and the ReceiveKey to the sender out of band
// (deep link, QR code, ...).
fn listen_for_payment(key: ReceiveKey) -> Result<Subscription<PaymentListenForItem>, ListenForPaymentError>;

enum PaymentListenForItem {
    Channel(PaymentTransmissionChannel),
    Cheque(Cheque),
}

enum PaymentTransmissionChannel {
    Standard {
        sss_topic: [u8; 32],
    },
}

enum ListenForPaymentError {
    ReceiveKeyNotFound,
    Other(GenericError),
}
```

#### Create a cheque

```rust
// Sender side: create a cheque that the product delivers to the recipient.
// Only the recipient can claim the items, but the cheque carries privacy-sensitive
// cleartext (item keys, recipient), so the transmission channel must add its own
// encryption layer.
// The items become InUse and stay InUse until the cheque is revoked with `revoke_cheque`.
// Asks for a confirmation if owner(from) != PurseOwner::Product(self).
fn create_cheque(
    from: PurseId,
    to: ChequeDestination,
    items: Vec<ItemKey>,
) -> Result<Cheque, CreateChequeError>;

enum CreateChequeError {
    Denied,
    PurseNotFound,
    ItemNotFound,
    ItemUnavailable,
    Other(GenericError),
}
```

#### Revoke cheque

```rust
// Release the items handed off in the cheque back to Idle.
// The product must be certain the handoff did not happen, i.e. the recipient
// does not know the cheque's secrets.
// After revocation the host rotates the item keys (allocates new keys and
// transfers the items to them) without product involvement. This is not optional.
// Revoking the same cheque twice is a no-op.
// Denied if `cheque_id` was not created by this product.
fn revoke_cheque(cheque_id: ChequeId) -> Result<(), RevokeChequeError>;

enum RevokeChequeError {
    // The cheque was not created by this product
    Denied,
    ChequeNotFound,
    Other(GenericError),
}
```

#### Deposit a cheque

```rust
// Receiver side: deposit a received cheque into `to`.
// Cheque.id is the idempotency key: depositing the same cheque again fails with OperationExists.
// Depositing a cheque with the same id but different content is undefined behavior.
// Needs no grant or confirmation: receiving only adds items to a purse.
fn deposit(what: Cheque, to: PurseId) -> Result<(), DepositError>;

enum DepositError {
    PurseNotFound,
    OperationExists,
    // Cheque.items is empty
    EmptyCheque,
    // Cheque.encrypted_secrets cannot be decoded, or the decoded secrets do not match `items`
    CorruptedCheque,
    Other(GenericError),
}
```

#### Refund a cheque

```rust
// Receiver side: return the items of a previously deposited cheque to their original keys.
// The cheque id is the idempotency key: refunding the same cheque again fails with OperationExists.
// Minted items that are no longer Idle are reported in Status::Incomplete.failed.
// Denied if `what` was not deposited by this product.
// Asks for a confirmation if the purse it was deposited into is not owned by PurseOwner::Product(self).
fn refund(what: ChequeId) -> Result<(), RefundError>;

enum RefundError {
    // The cheque was not deposited by this product
    Denied,
    ChequeNotFound,
    OperationExists,
    Other(GenericError),
}
```

### Transfer item

```rust
// Transfer a purse item to `destination`.
// Asks for a confirmation if owner(purse(item)) != PurseOwner::Product(self).
fn transfer_item(
    item: ItemKey,
    destination: ItemKey,
    key: IdempotencyKey,
) -> Result<(), TransferItemError>;

enum TransferItemError {
    Denied,
    // The item is not in any known purse
    ItemNotFound,
    // The item is not Idle
    ItemUnavailable,
    // An operation with `key` is already registered
    OperationExists,
    Other(GenericError),
}
```

### Backup recovery

The host's only obligation during backup recovery is to recover all assets. In addition:

- The host SHOULD allocate purses and purse items so that they can be enumerated on recovery.
- The host MAY recover purse metadata (name, owning product). A purse whose metadata is not recovered SHOULD be owned
  by `PurseOwner::Root`, and the host SHOULD give it a name of its choice.
- The host MAY recover handoff marks and the information needed to rebuild `InUse` statuses.

## Drawbacks

- Products cannot sign arbitrary transactions with purse items. Any external use of an item beyond `transfer_item`
  needs a new host call.
- `revoke_cheque` relies on the product's claim that the recipient never saw the cheque. A wrong claim lets both
  sides attempt to claim the same items; the mandatory key rotation limits but does not remove that race.
- Since `InUse` statuses may be lost on backup recovery, items handed off in an undeposited cheque come back as `Idle`
  and can be spent again by the sender, racing the recipient's deposit.
- Recovered purses without metadata become root-owned, so the product that created them loses default access and has
  to go through the permission flow.

## Testing, Security, and Privacy

- Products never see item secrets. Secrets leave the host only inside `Cheque.encrypted_secrets`.
- A cheque's cleartext part links item keys and, for `Ephemeral`, the receive key. Products must transmit cheques
  over an encrypted channel.

## Prior Art and References

- [RFC 0017: Coinage Payment User Agent API](0017-coinage-payment.md), which this RFC is based on.
- [RFC 0022: Account key derivations](0022-account-derivations.md), which defines the X25519 keys used by
  `ChequeDestination::Static`.
- [RFC 0002: Permission Model for Host API](0002-permission-model.md).

## Future Directions

- A device transfer feature, which changes the user's primary device and moves local data to the new one, could also
  transfer handoff marks and purse metadata (owning product and name). Unlike backup recovery, purses would then keep
  their owners and names, and items handed off in undeposited cheques would stay `InUse`.
