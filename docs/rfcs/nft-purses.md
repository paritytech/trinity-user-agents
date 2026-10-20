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
- every chain-exposed operation takes an idempotency key, and repeating a call returns the status of the operation
  already started, so a product can recover the outcome after a restart;
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
- Chain-exposed operations are asynchronous and idempotent. Repeating a call with the same idempotency key returns the
  status of the existing operation.
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
    // Purse is owned by the root: no product can access it without permission.
    // The main purse is root-owned, and so are purses recovered without metadata
    // (see "Backup recovery").
    Root,
}
```

A purse created with `create_purse` is owned by the calling product. Calls marked "subject to permission check" are
allowed without a check when the caller owns the purse. Otherwise they require the `NftPurse` user-agent permission,
granted per product, purse and action in the same way as RFC 0017's `CoinPayment`:

```rust
enum UserAgentPermission {
    // existing variants...
    NftPurse,
}
```

A `CoinPayment` grant does not cover NFT purses, and an `NftPurse` grant does not cover coin purses.

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
// Subject to permission check if owner(purse) != PurseOwner::Product(self).
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
// Subject to permission check if owner(purse) != PurseOwner::Product(self).
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
// Subject to permission check if owner != PurseOwner::Product(self).
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
// The unordered set of `keys` together with `to` is the idempotency key: a repeated
// call with the same set and `to` returns the status of the operation already started.
// A call whose `keys` overlap an active operation's keys with a different set or a
// different `to` fails with ItemUnavailable.
// The items are InUse while the operation is active.
// Subject to permission check if owner(from) != PurseOwner::Product(self).
fn rebalance_purse(
    from: PurseId,
    to: PurseId,
    keys: Vec<ItemKey>,
) -> Result<Subscription<Status>, RebalancePurseError>;

enum RebalancePurseError {
    Denied,
    // `from` or `to` was not found
    PurseNotFound,
    // Some item in `keys` was not found in `from`
    ItemNotFound,
    // Some item in `keys` is not Idle
    ItemUnavailable,
    Other(GenericError),
}
```

#### Operation status

`Status` is shared by `rebalance_purse`, `deposit`, `refund` and `transfer_item`.

```rust
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
    // Trie root of `leaves`
    root: [u8; 32],
    // Source item and the transaction that moved it
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
// Not subject to a permission check: receiving only adds items to a purse.
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
// Subject to permission check if owner(from) != PurseOwner::Product(self).
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
// Cheque.id is the idempotency key: depositing the same cheque again is a no-op
// and returns the status of the existing deposit.
// Depositing a cheque with the same id but different content is undefined behavior.
// Not subject to a permission check: receiving only adds items to a purse.
fn deposit(what: Cheque, to: PurseId) -> Result<Subscription<Status>, DepositError>;

enum DepositError {
    PurseNotFound,
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
// The cheque id is the idempotency key: refunding the same cheque again is a no-op
// and returns the status of the existing refund.
// Minted items that are no longer Idle are reported in Status::Incomplete.failed.
// Denied if `what` was not deposited by this product.
fn refund(what: ChequeId) -> Result<Subscription<Status>, RefundError>;

enum RefundError {
    // The cheque was not deposited by this product
    Denied,
    ChequeNotFound,
    Other(GenericError),
}
```

### Transfer item

```rust
// Transfer a purse item to `destination`.
// (item, destination) is the idempotency key: a repeated call with the same pair
// returns the status of the existing operation. A call with the same item and a
// different destination fails with ItemUnavailable.
// Subject to permission check if owner(purse(item)) != PurseOwner::Product(self).
fn transfer_item(
    item: ItemKey,
    destination: ItemKey,
) -> Result<Subscription<Status>, TransferItemError>;

enum TransferItemError {
    Denied,
    // The item is not in any known purse
    ItemNotFound,
    // The item is not Idle
    ItemUnavailable,
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
