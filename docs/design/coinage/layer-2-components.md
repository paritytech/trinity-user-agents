---
title: "Coinage payments — layer 2 component model"
status: "Draft"
---

# Coinage payments — layer 2

Layer 2 builds the two interfaces that products use on top of layer 1 ([`coinage-layer-components.md`](coinage-layer-components.md)), which provides purses, export and import of coins, and operation status. The two interfaces are RFC-6 (balance, top-up, payment requests, status) and RFC-17 (purses, receivables, cheques, invoices, deposits, refunds). [`rfc-deviations.md`](rfc-deviations.md) records every place where layer 2 departs from either RFC.

Section 3 describes the components and their contracts. Section 5 argues that the components together give the payment system the properties it needs. Section 4 lists the decisions that are still open.

In this document, **layer 1** means the coinage layer, and **layer 2** means the layer described here. The **payer** and the **payee** are user agents. A **product** is an application that calls one of these user agents.

## 1. What layer 2 does not own

| Interface | What it supplies |
|---|---|
| **Layer 1** | Purses, export, import, reclaim, offload, top-up, operation status (layer 1, G1) |
| **Product context** | The authenticated identity of the calling product |
| **Consent** | The core's permission service, which asks the user once, stores the answer per product and lets the user revoke it in the host's settings, and its one-off confirmation of a single action (J1) |
| **Transport** | Posting bytes under a topic and reading them back, for as long as the transport retains them. A post is signed with the statement-store key of the product the operation belongs to (M1) |
| **Randomness** | Receivable keys, and the ephemeral key and the nonce that seal a cheque (H3) |
| **Clock** | The current time, kept to network time |
| **Store** | The [shared store](store-contract.md). Layer 2's records are kept in the same store as layer 1's records. The store is never restored, and layer 2's records are never lost unless layer 1's records are lost too |
| **Parameters** | The claim timeout, the refund window and the clock tolerance, which each implementation sets within protocol-wide bounds (RFC deviations, gap 5). Also how long a top-up waits for its source to have the funds |

Layer 2 runs where layer 1 runs, on the device that holds the seed (layer 1, section 6). A host paired with that device answers every RFC-6 and RFC-17 call as unsupported. On the device that holds the seed, a call made while the layers start waits for the start (layer 1, G3), and is answered as unsupported only if they do not start.

The chat does not use layer 2. It has its own transport and its own encryption, it needs no receivable and no product permission, and it calls layer 1 directly, through the core's native interface ([`native-interface.md`](native-interface.md)), as the app's other wallet screens do.

Three rules apply to every component:

- **Layer 2 holds a coin secret only while it handles it.** An export's secrets go directly into a cheque. An import's secrets come from a cheque or a top-up and go directly to layer 1.
- **A deposit, a refund or a reclaim that layer 2 starts in layer 1 has a key derived from the operation's own inputs**, such as a cheque's hash. The key is hashed under a label that names the kind of operation, in one layer-1 scope reserved for layer 2. Because of the label, a deposit and a reclaim of the same cheque never share a key, even in a payment between two products of one user agent. The key is `OpKey { scope: "coinage/layer-2", id: BLAKE2b-256(label ‖ SCALE(inputs)) }`, with these labels and inputs: a deposit, `"coinage/deposit"` and the cheque's hash; a refund, `"coinage/refund"`, the deposit's cheque hash and the number of earlier refunds of that deposit that have ended (K3 invariant 2); a reclaim, `"coinage/reclaim"`, the cheque's hash and the number of earlier reclaims of that cheque that have ended (K4 invariant 3). RFC-6 payments and top-ups are different: their key is the id that their product chose, in the product's own scope (N2 invariant 3). Three operations run under a random key instead: a payment's export (K1), and RFC-17's `rebalance_purse` and `delete_purse`.
- **Layer 2 records an operation before starting it.** The record names the operation's key. Suppose layer 2 crashes after writing the record and before starting the operation. After the restart, either layer 1 never started the operation, and the flow starts it under the same key, or layer 1 already started it, and the flow joins it (layer 1, B4). If layer 1 refuses to start the operation, the refusal is written into the same record, and nothing tries to start it again. `NeedsConfirmation` is not such a refusal: layer 1 answers it before recording anything, the surface asks the user, and the operation is started again under the same key, with `confirmed` set if the user agrees (N1, N2). So a crash never produces a second operation, and never an operation that layer 2 has no record of. The three random-key operations are the exceptions. `rebalance_purse` and `delete_purse` are not recorded at all. A payment's export is recorded only after its secrets are sealed, which is safe because an export that is not yet committed returns to the purse on restart (layer 1, G1 invariant 3).

Layer 2 is constructed with the running layer 1 (layer 1, G1 `Layer`), the interfaces above and the parameters, and holds them in memory for as long as it runs. Open streams are held in memory and end at a restart, as in layer 1 (layer 1, section 2), and an entry's **Owns** lists only what a component holds beyond them.

Layer 2's records are in layer 1's store so that they cannot be lost while layer 1's records survive. If that happened, layer 1 would hold committed exports that nothing would ever reclaim.

**One entry point drives both layers.** `run_due(now)` first runs the work of layer 2 that is due, then the work of layer 1 that is due (layer 1, G1 `run_due`). Layer 2's work is done by three flows, each through its own `run_due(now)`: K5 reclaims cheques whose claim deadline has passed and collects refunds; K2 starts the automatic refund of each failed deposit, once (K2 invariant 5); K3 posts refund cheques that were recorded but not yet posted, until their claim deadlines. `run_due` returns layer 1's report of what it could not tell, and the earliest of the next due times that the three flows and layer 1 report. While the layers are running, the core follows the People chain's finalized heads, and each new head counts as a call: layer 1 observes it (layer 1, F1, which also announces it as `Event::NewBlock`), and the core then calls `run_due` (layer 1, F2 invariant 2). No host callback is involved, and the core subscribes again when the node's subscription ends. The host's own periodic call covers the time when the layers are not running.

## 2. Dependency order

A component may depend only on components above it. Every component may call layer 1.

```
H1 Receivable keys   H2 Topics   H3 Cheque   H4 Clearing reference   H5 Status mapping
                                      │
                                      ▼
            J1 Purse ownership and permissions      J2 Payment records
                                      │
                                      ▼
                                 M1 Channel
                                      │
                                      ▼
        K1 Pay    K2 Receive    K3 Refund    K4 Cancel    K5 Outgoing cheques
                                      │
                                      ▼
           N1 RFC-17 surface      N2 RFC-6 surface      N3 Invoice payment
```

## 3. Component catalogue

### Band H — Vocabulary
*Pure.*

---

#### H1 · Receivable keys

**Owns** nothing. **Band:** pure.

A **receivable** is an X25519 [RFC 7748] keypair. Its public half (32 bytes) names a payment, and the payment's cheque is sealed to it. The user agent keeps the secret half. A receivable has two uses: as the payee's receivable behind an invoice, and as the payer's **return receivable**, which is carried inside a cheque so that a refund can be sent back.

```rust
type Receivable       = [u8; 32];   // the X25519 public key
type ReceivableSecret = [u8; 32];   // the X25519 secret key

fn new_receivable(draw: [u8; 32]) -> (Receivable, ReceivableSecret);
```

- **`new_receivable`** turns 32 bytes drawn from the randomness of section 1 into an X25519 keypair: the secret is the draw, clamped as RFC 7748 specifies, and the receivable is its public key. The caller draws the bytes and records the receivable, its secret and its purse in J2 (J2 invariant 5): K1 for a return receivable, N1 for `create_receivable`. Fixed test vectors supply the draw.

Receivables are drawn at random rather than derived from the seed, because no value is lost if a receivable cannot be recovered. A cheque that nobody can open stays with its sender (the payer, or the payee for a refund), and the sender takes the coins back after the claim deadline (K5).

**Invariants**
1. Every receivable is a fresh draw from the randomness of section 1, never derived from the seed or from another receivable.

**What it relies on.** Each receivable is used for one payment flow only: one invoice, or the return path of one cheque (K1 step 3, N1 `create_receivable`). If a receivable were used twice, its topic (H2) would link the two payments.


---

#### H2 · Topics

**Owns** nothing. **Band:** pure.

```rust
type Topic = [u8; 32];

fn payment_topic(receivable: Receivable) -> Topic;
fn refund_topic(return_receivable: Receivable) -> Topic;
```

- **`payment_topic`**: where a payee listens for the cheque.
- **`refund_topic`**: where a payer listens for a refund.

Both functions hash the receivable's 32 bytes together with a label, and the two labels differ:

```text
payment_topic = BLAKE2b-256("coinage/payment-topic" ‖ receivable)
refund_topic  = BLAKE2b-256("coinage/refund-topic"  ‖ return_receivable)
```

Each label is hashed as its ASCII bytes. A topic is 32 bytes long, which is the topic size the statement store accepts.

**Invariants**
1. A payment topic never equals a refund topic, whatever the receivables.
2. A topic is derived from the receivable alone, so it reveals nothing that the receivable does not reveal: no purse, no product and no amount.

---

#### H3 · Cheque

**Owns** nothing. **Band:** pure.

```rust
struct Cheque {                    // RFC-17's CoinPaymentCheque, field for field
    to: Receivable,                // the payee's receivable; the wire's `id`
    amount: u32,                   // RFC-17's Balance, in cents
    sealed: Vec<u8>,               // ChequeContents, sealed to `to`; the wire's `encrypted_secrets`
}
struct ChequeContents {
    coins: Vec<(CoinSecret, Exponent)>,
    return_to: Option<Receivable>, // the payer's return receivable; none on a refund
    claim_by: u64,                 // the claim deadline, Unix milliseconds of network time
}
type ChequeHash = [u8; 32];

enum SealError {
    LowOrderReceivable,            // invariant 1
    AmountMismatch,                // invariant 3
}
enum OpenError {
    LowOrderReceivable,            // invariant 1
    UnknownVersion,
    NotAuthentic,                  // tampered with, or sealed to another receivable
    Undecodable,                   // the plaintext is not SCALE(ChequeContents)
}

fn seal(to: Receivable, amount: u32, contents: ChequeContents,
        ephemeral: [u8; 32], nonce: [u8; 12]) -> Result<Cheque, SealError>;
fn open(secret: ReceivableSecret, cheque: Cheque) -> Result<ChequeContents, OpenError>;
fn cheque_hash(cheque: Cheque) -> ChequeHash;
```

- **`seal`** builds the cheque as **Sealing** below describes. `ephemeral` and `nonce` are drawn by the caller (K1, K3) from the randomness of section 1, so `seal` itself is pure and fixed test vectors can supply them. It refuses a low-order `to`, and contents whose coins, each worth `2^exponent` cents, do not add up to `amount`.
- **`open`** recovers the contents with the receivable's secret. It fails on a low-order `to`, an unknown version byte, a seal that does not authenticate, or a plaintext that does not decode. It does not compare the coins with `amount`: K2's `deposit` does that, because only the receiver can be sure the sender used an honest `seal`.
- **`cheque_hash`** is BLAKE2b-256 [RFC 7693] of `SCALE(Cheque)`. It identifies a cheque wherever layer 2 keys or looks up something by cheque: deposits (K2 invariant 1), outgoing cheques and their reclaims (J2, K4 invariant 3).

**Invariants**
1. Only the holder of the receivable's secret can open a cheque. Sealing is authenticated: a cheque that was tampered with fails to open, instead of opening to different coins. `seal` refuses, and `open` rejects, a `to` whose key agreement gives an all-zero shared secret. Such a `to` is a low-order point, and with it anyone could derive the key.
2. `to` and `amount` are bound into the seal, so a cheque cannot be redirected to another receivable, and its amount cannot be changed.
3. `amount` equals the value the contents declare for their coins.
4. The return receivable is inside the seal. So only the payee learns the return receivable, and with it the refund topic (H2).
5. **A payment cheque always carries a return receivable (K1), and a refund cheque never does (K3).** So a refund cannot itself be refunded. The kind of a cheque also follows from the topic it arrives on: the payment topic or a refund topic (H2). Refund cheques never reach a product. So a cheque needs no separate field for its kind.
6. **The cheque carries its claim deadline**: the payer's send time plus the payer's claim timeout. So payer and payee use the same instant, even when their own claim timeouts differ.

**Sealing** uses the same cryptographic primitives as the chat, so it introduces no new cryptography:
- the payer's ephemeral X25519 secret is `ephemeral`, and the shared secret is its key agreement with `to`;
- the key is BLAKE2b-256 [RFC 7693] over the shared secret, the ephemeral public key and `to`;
- the contents are encrypted with ChaCha20-Poly1305 [RFC 8439] under `nonce`, with `SCALE((version, to, amount))` as associated data, `version` being 1.

`sealed` is the version byte, then the ephemeral public key, then the nonce, then the ciphertext. So the cheque keeps RFC-17's three fields. A cheque with an unknown version fails to open. The ephemeral key is used once and then discarded, so a cheque contains no long-lived key of the payer. `ChequeContents` and `Cheque` are SCALE-encoded, using layer 1's types (layer 1, G1).

---

#### H4 · Clearing reference

**Owns** nothing. **Band:** pure.

Builds RFC-17's `ClearingReference`: one leaf per claimed coin, and a Merkle root over the leaves. A leaf names the coin's account and the transaction that moved the coin. For a deposit, K2 takes the transactions from the import's completion. For a refund, the payer claims the coins, so K3 takes the transactions from the coins' export status (`Claimed { by }`, layer 1, B1).

```rust
struct Leaf { coin: AccountId, transaction: TxHash }
struct ClearingReference { root: [u8; 32], leaves: Vec<Leaf> }   // RFC-17's CoinPaymentClearingReference
struct InclusionProof { position: u32, leaf_count: u32, siblings: Vec<[u8; 32]> }

fn build(leaves: Vec<Leaf>) -> ClearingReference;
fn prove(reference: ClearingReference, coin: AccountId) -> Option<InclusionProof>;   // None: no leaf for that coin
fn verify(root: [u8; 32], leaf: Leaf, proof: InclusionProof) -> bool;
```

- **`build`** sorts the leaves and computes the root as **Construction** below says. `build` of no leaves is the **empty reference**: an all-zero root and no leaves (RFC deviations, entry 14).
- **`prove`** gives the inclusion proof for the leaf of one coin.
- **`verify`** is what a third party runs: it recomputes the root from the leaf and the proof and compares. Checking on chain that the leaf's transaction is finalized and moved the leaf's coin is the third party's own step (invariant 2).

**Invariants**
1. A reference is built only from outcomes that reached a stable verdict, and it names only finalized blocks (layer 1, B4 invariant 4).
2. **A third party with chain access can verify every leaf**: that the transaction the leaf names is finalized, and that this transaction moved the leaf's coin. Verification is the purpose of the reference. It is also the reason the reference reveals the payer's coins (RFC deviations, gap 9).
3. **One leaf can be proved without the others.** The holder shows a third party the root, the leaf and its inclusion proof, and reveals no other coin.

**Construction**, which every verifier reproduces:
- a leaf is SCALE `(CoinPublicKey, TransactionHash)`;
- leaves are sorted by their encoding, so every party computes the same root;
- the root is a binary Merkle tree over BLAKE2b-256 [RFC 7693]. A leaf's node is the hash of the leaf's encoding. Each row pairs adjacent nodes from left to right, and a pair's parent is the hash of the left node's 32 bytes followed by the right node's 32 bytes. An unpaired last node moves up to the next row unchanged. The one node left at the top is the root; with no leaves, the root is 32 zero bytes;
- an inclusion proof for one leaf gives the leaf's position, the number of leaves, and the sibling node at each row where the leaf's node has one. This construction is the one Polkadot SDK's `binary-merkle-tree` library implements [binary-merkle-tree].

---

#### H5 · Status and error mapping

**Owns** nothing. **Band:** pure.

Translates layer 1's statuses, failures and per-coin results into RFC-17's and RFC-6's vocabularies. The RFC types are used as RFC-17 and RFC-6 define them: RFC-17's `CoinPaymentStatus`, `CoinPaymentError` and `CoinPaymentChequeProgress` (RFC deviations, entry 3), and RFC-6's payment status, top-up status and payment error. RFC-6's amounts are in the asset's units (N2 invariant 5): `payment_status` and `top_up_status` take the amount the product asked for in those units, and `unit`, the asset's amount in one cent (layer 1, G1 `asset_unit`), by which they convert layer 1's cents, saturating.

```rust
fn deposit_status(item: StatusItem, coins: Vec<(AccountId, CoinResult)>, expected: Amount,
                  reference: ClearingReference) -> CoinPaymentStatus;
fn own_move_status(item: StatusItem) -> CoinPaymentStatus;                 // a rebalance or a deletion
fn cheque_progress(coins: Vec<(Exponent, ExportStatus)>) -> CoinPaymentChequeProgress;
fn refund_status(cheques: Vec<CoinPaymentChequeProgress>, missing_return_path: bool,
                 reference: ClearingReference) -> CoinPaymentStatus;
fn cancellation_status(cheque: CoinPaymentChequeProgress) -> CoinPaymentStatus;
fn coin_payment_error(refusal: Refusal) -> CoinPaymentError;
fn payment_status(item: StatusItem, amount: u128, unit: u128) -> PaymentStatus;   // RFC-6 `status_subscribe`
fn top_up_status(item: StatusItem, amount: u128, unit: u128) -> TopUpStatus;      // RFC-6 `top_up_status_subscribe`
fn payment_error(refusal: Refusal, balance_access: bool) -> PaymentError; // RFC-6 `request`

enum Refusal { Layer1(Error), ConsentRefused }
```

- **`deposit_status`**, **`own_move_status`**, **`payment_status`**, **`coin_payment_error`** and **`payment_error`** follow the first table. A deposit's `Done` needs `item` at `Done` and its settled value equal to `expected` (K2 invariant 4); a failed deposit's error comes from its coins' results. `own_move_status` carries the empty reference (H4).
- **`top_up_status`** follows the paragraph on top-ups below.
- **`cheque_progress`** sums each coin's value, `2^exponent` cents, into the four amounts of the second table. K1 serves it to the payer (K1 `cheque_status`).
- **`refund_status`** and **`cancellation_status`** follow invariant 4.

| Layer 1 | RFC-17 | RFC-6 |
|---|---|---|
| `Preparing`, `Waiting`, `Submitted`, `Settling` | `Clearing { clearing: in_flight, cleared: settled }` | `Processing` |
| `Done` | `Done { cleared, reference }`, for a deposit only if `cleared` is the cheque's amount (K2 invariant 4) | `Completed`, if the full amount reached the destination; otherwise `ClaimedPartially { actual_claimed }` (N2 invariant 5) |
| `PartiallyDone`, `Failed`, or a deposit's `Done` at another amount | `Failed { error, cleared, reference }` | `ClaimedPartially { actual_claimed }` if any of it reached the destination, otherwise `Failed` |
| Coin result `Absent` or `NotClaimed` | `BadCoins` | — |
| Coin result `TakenByOther` | `SnipedCoins` | — |
| `InsufficientFunds` | `BalanceLow` | `InsufficientBalance` to a caller with balance access, otherwise `Rejected` (RFC-6) |
| `NeedsConfirmation` | Not returned: N1 asks the user, and a refusal is `Denied` | Not returned: N2 asks the user, and a refusal is `Rejected` |
| `NoUnloadToken` | `Internal` | `Unknown { reason }` |
| `UnsupportedRuntime`, `Unavailable` | `UserAgentCapabilityUnavailable` | `Unknown { reason }` |
| `InvalidSecret` | `BadCoins` | `InvalidSource` (N2 invariant 4) |
| A refund for a deposit with no return path (K3 invariant 3) | `Failed { error: Internal }`, until RFC-17 has a variant for it (RFC deviations, gap 5) | — |
| Consent refused | `Denied` | `Rejected` |

**A top-up's RFC-6 status** follows its layer-1 operation. It is `Detecting` while the operation is `Waiting`, and `Claiming` while the operation is `Preparing` or `Submitted`. It is `Claimed { finalized: false }` once the value that succeeded, on the best chain or finally, reaches the amount. It is `Claimed { finalized: true }` once layer 1 reports `Done` at the amount. At any other terminal status, it is `ClaimedPartially { actual_claimed }` if any value settled, and `NotClaimed` if none did. A reorganisation may move `Claimed { finalized: false }` back to `Detecting`, and G2 invariant 3 of layer 1 requires that this change is shown.

The `Done` and `Failed` statuses of a rebalance, a deletion or a cancellation carry the empty reference: an all-zero root with no leaves (RFC deviations, entry 14).

**A cheque's progress**, on the payer's side, sums the value of the cheque's coins by their export status (layer 1, B1):

| Export status | Counted as |
|---|---|
| `NotYetMinted` | `minting` |
| `AwaitingPayee`, `Claimed { stable: false, .. }`, `Reclaimed { stable: false }` | `claimable` |
| `Claimed { stable: true, .. }` | `claimed` |
| `Reclaimed { stable: true }`, `NeverMinted` | `returned` |

**Invariants**
1. **The mapping never reports more progress than layer 1 did.** Nothing maps to `Done` or `Completed` unless layer 1 reported `Done`. So layer 1's guarantee that no outcome is reported as more settled than it is (layer 1, P11) also holds for the RFC statuses.
2. **A refusal never reveals a balance.** A product without balance access is told that its request was denied, not that the balance was too low. The shipped wallet already behaves this way (RFC deviations, entry 15).
3. **A cheque's progress counts a claim only once the claim is stable** (invariant 1). The progress does not predict when anything will happen.
4. **The RFC-17 status of a refund or a cancellation is derived from the progress of its cheques.** It is `Clearing` while any value is minting or claimable. After that, it is `Done` if all the value reached the side the call wants: claimed by the payer for a refund (K3 invariant 5), or returned for a cancellation (K4). Otherwise it is `Failed`, and for a cancellation the error is `SnipedCoins`. A refund under which some deposit had no return receivable (K3 invariant 3) ends `Failed { error: Internal }`, even if every cheque the refund sent was claimed.

---

### Band J — Records
*Stateful, effect-free.*

---

#### J1 · Purse ownership and permissions

**Owns** nothing: the creator of a purse is its layer-1 tag (invariant 2), and every answer the user gives is stored by the core's permission service (section 1). **Band:** stateful, effect-free.

```rust
type ProductId = Vec<u8>;          // the product's authenticated identity, as the core runtime passes it (section 1)
enum Action { Query, Receive, Spend, TopUp }
enum Decision { Allowed, Ask(Question), Denied }
enum Question {
    CoinPayment,                               // a stored permission: RFC-17's CoinPayment permission
    BalanceAccess,                             // a stored permission: RFC-6's balance access
    Purse { purse: PurseId, action: Action },  // a stored permission: one action on a purse another product created
    Spend { purse: PurseId, amount: Amount },  // a one-off confirmation of one spend
}

fn authorize(product: ProductId, purse: PurseId, action: Action, amount: Amount, rfc6: bool) -> Decision;
```

- **`authorize`** decides one call from the purse's creator and the permissions the permission service holds for `product`, by the first rule that applies:
  1. Without the `CoinPayment` permission, `Ask(CoinPayment)`, or `Denied` if the user refused it, except for an RFC-6 call on the main purse (invariant 5).
  2. A top-up is `Allowed` (invariant 3).
  3. A query or a receive on a purse the product created is `Allowed` (invariant 1).
  4. An RFC-6 query of the main purse needs RFC-6's balance access: `Ask(BalanceAccess)` while none is stored, `Denied` if the user refused it. N2 asks for the same permission before it calls `authorize`, so the user is asked once (invariant 6).
  5. A query, a receive or a spend on a purse another product created, or on the main purse, needs the stored permission for that purse and action: `Ask(Purse { purse, action })` while none is stored, `Denied` if the user refused it.
  6. A spend that passed rule 5, or on the product's own purse, is `Ask(Spend { purse, amount })` (invariant 4).

  N1 and N2 put an `Ask` to the user: a stored permission through the permission service, which stores the answer, and a spend through the one-off confirmation, which stores nothing. A refusal is `Denied`.

The permission service already holds a product's permissions per product and kind, asks once, and shows each permission in the host's settings, where the user can revoke it. `CoinPayment` and `Purse { purse, action }` are two kinds of permission in it, and RFC-6's existing balance access is a third, which N2 reads for `balance_subscribe`. When the user revokes one, N1 and N2 close every open stream it authorized (N1 invariant 5).

**Invariants**
1. By default, the product that created a purse may query it and receive into it. Any other product needs the user's permission (RFC-17). The main purse has no creator, so every product needs a permission for it.
2. **The creator is stored as the purse's layer-1 tag**, which is written in the same commit that creates the purse (layer 1, B2). So no purse exists without its creator recorded.
3. A top-up only adds value for the user, so it needs no consent (RFC-6).
4. **Every spend is confirmed by the user**, whoever created the purse. Whether a spend needs explicit confirmation is the user agent's decision, not the product's (RFC-17), and this user agent always asks.
5. **No product reaches a purse without RFC-17's `CoinPayment` permission.** The shipped wire has no call that requests this permission, so the user agent asks the user on the first call that needs it (RFC deviations, entry 17). RFC-6 calls on the main purse do not need this permission.
6. **One question per fact.** Reading the main purse's balance through RFC-6 needs RFC-6's balance access and nothing else, so the user is never asked twice to let a product see the balance.

---

#### J2 · Payment records

**Owns** layer 2's records of payments. **Band:** stateful, effect-free.

Every record that names a layer-1 operation is written before that operation starts, under the operation's key (section 1). If layer 1 refuses to start it, the refusal is written into the same record.

```rust
struct OperationRecord { key: OpKey, refused: Option<Error> }   // section 1: recorded before it starts

struct ReceivableRecord {
    receivable: Receivable,
    secret: ReceivableSecret,
    purse: PurseId,                     // invariant 5
    return_receivable: bool,            // drawn by K1 as a payment's return receivable; K2 never refunds under it
    deposits: Vec<DepositRecord>,
}
struct DepositRecord {
    cheque: ChequeHash,
    product: ProductId,                 // the product that deposited it; its statement-store key posts the refunds (M1)
    amount: Amount,
    claim_by: Time,
    import: OperationRecord,
    return_to: Option<Receivable>,
    refunds: Vec<RefundRecord>,         // in the order they were started
    automatic_refund_made: bool,        // K2 invariant 5
}
struct RefundRecord { operation: OperationRecord, cheque: Option<ChequeHash> }   // the refund cheque, once sealed (K3)

struct OutgoingCheque {
    hash: ChequeHash,
    cheque: Cheque,                     // the sealed cheque, kept so it can be posted again
    source: PurseId,
    payee: Receivable,
    export: OpKey,
    return_to: Option<Receivable>,      // none for a refund cheque
    claim_by: Time,                     // the refund deadline follows from it (K5)
    product: ProductId,                 // the product it belongs to: the one that paid, or the one the refund was made for
    post: bool,                         // a refund cheque, which layer 2 posts with the product's statement-store key (M1); a payment cheque is handed over by its product
    posted: bool,
    reclaims: Vec<OperationRecord>,     // in the order they were started (K4, K5)
}
struct Rfc6Record {
    product: ProductId,
    id: [u8; 32],
    kind: Rfc6Kind,                     // Payment or TopUp
    amount: u128,                       // the amount asked for, in the payment asset's units (N2 invariant 5)
    source: Option<AccountId>,          // a top-up's paying account; a private key itself is never stored
    funding: Option<String>,            // the funding session it serves, when a funding provider's worker started it (N2 invariant 6)
    operation: OperationRecord,
}

// writes, each inside the caller's store transaction
fn record_receivable(txn: StoreTxn, record: ReceivableRecord);
fn record_deposit(txn: StoreTxn, receivable: Receivable, deposit: DepositRecord);
fn record_refund(txn: StoreTxn, deposit: ChequeHash, refund: RefundRecord);
fn record_outgoing(txn: StoreTxn, cheque: OutgoingCheque);
fn record_reclaim(txn: StoreTxn, cheque: ChequeHash, reclaim: OperationRecord);
fn record_rfc6(txn: StoreTxn, record: Rfc6Record) -> Result<(), AlreadyExists>;   // N2 invariant 3
fn record_refusal(txn: StoreTxn, key: OpKey, error: Error);
fn mark_posted(txn: StoreTxn, cheque: ChequeHash);
fn mark_automatic_refund(txn: StoreTxn, deposit: ChequeHash);

// reads
fn receivable(receivable: Receivable) -> Option<ReceivableRecord>;
fn deposit(cheque: ChequeHash) -> Option<(Receivable, DepositRecord)>;
fn outgoing(cheque: ChequeHash) -> Option<OutgoingCheque>;
fn unposted(now: Time) -> Vec<OutgoingCheque>;        // refund cheques recorded, not posted, claim deadline not passed
fn rfc6(product: ProductId, id: [u8; 32]) -> Option<Rfc6Record>;
fn ended(records: Vec<OperationRecord>) -> u32;       // how many of them were refused or have a layer-1 completion
fn refundable(deposit: ChequeHash) -> Amount;         // invariant 1
```

- The **writes** add records or set a flag. None of them changes or removes an earlier record (invariant 3). `record_rfc6` refuses a `(product, id)` that is already recorded.
- **`unposted`** is what section 1's `run_due` posts again: the cheques layer 2 posts itself (`post`), which are refund cheques, recorded but not yet posted, until their claim deadlines.
- **`ended`** counts, among the given operation records, those that layer 1 refused and those whose layer-1 operation has a completion (layer 1, G1 `completion`). K3 keys a new refund, and K4 and K5 key a new reclaim, by this count over the deposit's refunds or the cheque's reclaims (K3 invariant 2, K4 invariant 3).
- **`refundable`** computes invariant 1 from the deposit's records and layer 1's results: the import's settled value from its completion, and each refund's claimed value from its cheque's export status (layer 1, B1).

The secrets J2 holds are receivable secrets, which protect no value on their own (H1), and sealed cheques, which only their payee can open. So J2 keeps no coin secret in the clear, as section 1 requires.

**Invariants**
1. **The value that may be refunded under a deposit is the value the deposit cleared, minus the value already refunded.** This amount never goes negative. Both sides of the subtraction count only stable outcomes. The cleared value is the import's settled value. A refund counts only the value its payer has claimed, and only once the claim is stable. Refund value that came back to the payee counts as nothing, so it can be refunded again.
2. Every record names, by its key, the layer-1 operation it started. So after a restart, the operation's status is read from layer 1 rather than reconstructed.
3. **Records are never deleted**, just as layer 1's records are not (layer 1, C1 invariant 1). So a cheque can be reclaimed, a refund counted and an invoice recognised as paid, however long ago the payment happened.
4. **A deleted purse resolves to the purse its value went to, as layer 1 resolves it** (layer 1, B2 `resolve_purse`). J2 keeps no record of its own for deleted purses. Anything that would land in, or leave from, a deleted purse, or a purse with an open deletion, uses the purse layer 1 resolves it to.
5. **A receivable's purse is recorded with the receivable and never changes.** Everything deposited under the receivable goes to that purse, or to the purse it resolves to (invariant 4).

---

### Band M — Transport

---

#### M1 · Channel

**Owns** nothing. **Band:** effectful.

```rust
enum ChannelItem {
    Inline(Cheque),      // whenever SCALE(Cheque) fits a statement
    Hop(HopReference),   // otherwise: HOP's reference to SCALE(Cheque)
}
enum PostError { Unreachable, TooLarge }

fn post(topic: Topic, cheque: Vec<u8>) -> Result<(), PostError>;   // cheque: SCALE(Cheque)
fn subscribe(topic: Topic) -> Stream<Vec<u8>>;
fn fetch(topic: Topic) -> Vec<Vec<u8>>;
```

The channel runs over the statement store [RFC-0008]. HOP, the Bulletin chain's peer-to-peer file transfer, is the fallback for payloads too large for a statement, as RFC-17 says [RFC-17]. The format of `HopReference` is not defined yet (section 4). A statement's data is `SCALE(ChannelItem)`, so every implementation posts and reads the same bytes.

- **`post`** puts the cheque's bytes under the topic: as `Inline`, whose encoding is its variant byte followed by the cheque's bytes, when that fits a statement, and otherwise through HOP as a `Hop` reference. It fails with `Unreachable` when neither transport accepts the post, and with `TooLarge` when the bytes fit neither. A failed post is posted again by section 1's `run_due` (J2 `unposted`).
- **`subscribe`** delivers the cheque bytes of every item under the topic that is still retained, then of every new one, following a `Hop` reference to its bytes. An item may be delivered more than once and in any order; receivers key what they do by the cheque's hash (K2 invariant 1).
- **`fetch`** returns the cheque bytes of every item under the topic that is still retained.

**Who signs.** A statement must be signed by an account that the statement store allows to post. Layer 2 signs with the statement-store key of the product the operation belongs to: for a refund, the product that asked for it, or the product that made the deposit for an automatic refund (J2). It is the key a product's own statements are signed with: derived from the seed under that product's identifier, and registered for each allowance period by the account holder. So a post by layer 2 reveals no more than a post by the product would, and it counts against that product's statement allowance for the period. Obtaining the key needs a connected session; while none is, the post fails with `Unreachable`, and `run_due` posts it again.

**Invariants**
1. The channel does not interpret the bytes it carries.
2. The channel keeps posted bytes only for a limited time.

---

### Band K — Flows
*Each component makes a pure decision and has an effectful part that calls layer 1 and the channel. None stores anything of its own: every record a flow writes is J2's.*

The flows use RFC-17's own types for what they return to a product: `CoinPaymentStatus` (`Clearing`, `Done`, `Failed`), `CoinPaymentError` (`BalanceLow`, `Denied`, `BadCoins`, `SnipedCoins`, `PurseNotFound`, `ReceivableNotFound`, `UnsupportedChannel`, `UserAgentCapabilityUnavailable`, `Internal`) and, for a cheque's progress, `CoinPaymentChequeProgress` (RFC deviations, entry 3). A cheque is named by its hash, `ChequeHash`, which H3 defines. Every layer-1 operation a flow starts has the key section 1 derives for it.

Three flows have work that comes due without any call: K2, K3 and K5 each offer `run_due(now)`, which section 1's entry point calls. Each returns the earliest time at which it next has work that the passage of time alone makes due, or none.

---

#### K1 · Pay

**Owns** nothing; its records are J2's. **Band:** pure decision with an effectful half.

```rust
enum PayError { Refused(CoinPaymentError), NeedsConfirmation }

fn pay(source: PurseId, to: Receivable, amount: u32,
       initiator: Initiator, confirmed: bool)            // layer 1, A4
    -> Result<Cheque, PayError>;
fn cheque_status(cheque: ChequeHash)
    -> Result<Stream<CoinPaymentChequeProgress>, CoinPaymentError>;
```

- **`pay`** runs steps 1 to 6 below and returns the sealed cheque. `amount` is in cents, like `Cheque.amount` (H3). If layer 1 refuses to start the export, `pay` records nothing and returns `Refused` with layer 1's error as H5 maps it (`InsufficientFunds` is `BalanceLow`, an unknown purse is `PurseNotFound`), or `NeedsConfirmation` when layer 1 answers that (layer 1, A4); the caller then asks the user (N1). The caller hands the cheque over, for example by posting it to the channel the payee's invoice names (RFC-17 `listen_for_payment`).
- **`cheque_status`** reads the outgoing cheque from J2 (`outgoing`) and follows layer 1's export status of the export the record names (layer 1, G1 `export_status`). It sums the value of the cheque's coins as H5's cheque-progress table says, emits the current progress first and then every change, and closes when no value is minting or claimable. A hash J2 does not hold as an outgoing cheque is refused with `ReceivableNotFound`, the answer RFC-17 gives for something the caller cannot see (N1 invariant 3). It works the same after a restart, because everything it reads is stored.

`pay` runs these steps:

1. The caller has authorized the spend: N1 through J1 (N1 invariant 1).
2. `export` from the source purse (layer 1), with the caller's initiator and confirmation, with transactions rebuilt until the claim deadline that the cheque will carry (`Retry::Until`, H3 invariant 6). The export runs under a random key (section 1). The coin secrets come back at once. The coins are minted afterwards, and the inputs of that minting may still be arriving.
3. Draw a fresh return receivable in the source purse (H1 `new_receivable`, with a draw from section 1's randomness), recorded in J2 as a return receivable.
4. Seal the cheque (H3 `seal`, with a fresh ephemeral key and nonce from section 1's randomness).
5. In one store transaction, record the outgoing cheque (J2 `record_outgoing`, with the sealed cheque) and call `commit_export` (layer 1, G1 invariant 3).
6. Return the cheque to the caller, who hands it over. From then on, `cheque_status` reports the cheque's progress (H5).

**Invariants**
1. **The record and the commit happen together, and both happen before the hand-over, without exception.** The record and the commit are one store transaction. So K5 knows every committed cheque, and no record names an export that was never committed. Both happen before the cheque exists anywhere outside layer 2. So no cheque is handed over while its coins can still be spent by this user agent.
2. A refund lands in the purse the payment came from, because the return receivable is bound to the source purse.
3. The cheque is ready as soon as step 5 commits. K1 waits for no chain transaction, because the payee's claim waits for the coins instead (K2 invariant 3).
4. **A payment is never refused because it has to wait** for value that will become usable without anyone acting (layer 1, B3). Inputs that are still settling delay the coins, not the cheque.
5. A call to pay takes no identifier. So a retry after a crash makes a new cheque. A cheque left behind by the crash was never handed over, and K5 reclaims it after its claim deadline. If sealing or recording fails without a crash, K1 releases the export (layer 1, `release_export`).

---

#### K2 · Receive

**Owns** nothing; its records are J2's. **Band:** pure decision with an effectful half.

```rust
fn listen(receivable: Receivable) -> Result<Stream<Cheque>, CoinPaymentError>;
fn deposit(cheque: Cheque) -> Result<Stream<CoinPaymentStatus>, CoinPaymentError>;
fn run_due(now: Time) -> Option<Time>;
```

- **`listen`** reports the cheques that arrive for a receivable J2 holds. It reads the receivable's payment topic: first everything still retained (M1 `fetch`), then each new item (M1 `subscribe`). It reports each item that decodes as a cheque whose `to` is the receivable, and ignores anything else. The same cheque may be reported more than once; depositing it again joins the first deposit (invariant 1). Receiving a cheque does not mean being paid (RFC-17). A receivable J2 does not hold is refused with `ReceivableNotFound`.
- **`deposit`** returns the deposit's status as H5 maps it: the current status first, then every change, ending at `Done` or `Failed` (invariant 4). If the cheque's hash names a deposit J2 holds, `deposit` joins it, before any check. Otherwise it checks the cheque, and refuses it before recording or claiming anything:
  - a `to` that J2 does not hold as a receivable: `ReceivableNotFound`;
  - a cheque that does not open (H3 `open` fails), whose declared coins do not add up to `amount`, or whose claim deadline lies further ahead than the protocol's maximum claim timeout plus the protocol's maximum clock tolerance (RFC deviations, gap 5): `BadCoins`. The deadline check ensures that no cheque keeps a deposit open indefinitely.

  A refused cheque leaves no record, so depositing it again checks it again. A cheque that passes is recorded (J2 `record_deposit`), and its coins are imported (layer 1 `import`) into the receivable's purse, resolved through any deletion (layer 1, B2), with `Retry::Until` the claim deadline.
- **`run_due`** starts the automatic refund (K3 `refund_deposit`) of every deposit that ended other than `Done` at the expected amount, whose automatic refund J2 does not yet record as made (invariant 5). A deposit under a return receivable is a refund the payer collected (K5), and is never refunded (H3 invariant 5). It returns none: a deposit's end is a layer-1 event, not a time.

**Invariants**
1. **A deposit is keyed by the cheque's hash.** So depositing the same cheque twice joins one import, and two claims of the same coins never compete with each other.
2. The coins go to the receivable's purse (J2 invariant 5), or to the purse that purse was drained into (J2 invariant 4), whatever the caller asks for. Neither the cheque nor the caller can send them elsewhere.
3. The import retries until the cheque's **claim deadline**, which the cheque carries (H3 invariant 6). The import waits for a coin that is not yet minted, instead of reporting it missing. Payer and payee use the same deadline on network time, so the payee stops claiming before the payer reclaims (K5). Each claim attempt is valid for at most the protocol's maximum claim validity window (RFC deviations, gap 5), because layer 1's longest validity window does not exceed that maximum (layer 1, A3 invariant 2). K5's waiting time relies on this bound.
4. **A deposit is `Done` only at the expected amount**: layer 1 must report the import done, and the import's settled value must equal the cheque's amount (H5 invariant 1). Any other outcome (a coin missing, a coin taken by someone else first, or a coin worth a different amount than declared) is `Failed`, with the value that cleared.
5. **A failed deposit refunds the value that cleared, automatically and once** (K3). So the payee keeps nothing it did not agree to, and needs no product to return it. The refund counts as made once its export is committed, or once layer 1 refuses to start it. If a restart released the refund's export before its commit, the refund is started again.

---

#### K3 · Refund

**Owns** nothing; its records are J2's. **Band:** pure decision with an effectful half.

```rust
fn refund(receivable: Receivable) -> Result<Stream<CoinPaymentStatus>, CoinPaymentError>;
fn refund_deposit(deposit: ChequeHash);
fn run_due(now: Time) -> Option<Time>;
```

- **`refund`** refunds, for a receivable J2 holds, everything still refundable under it (J2 invariant 1), as one refund cheque per deposit. It calls `refund_deposit` for every deposit with a refundable value, which joins any refund of that deposit in progress (invariant 2). It returns the status H5 invariant 4 derives from the progress of all those refund cheques. If nothing is refundable, the stream ends at once with `Done`, zero value cleared and the empty reference (RFC deviations, entry 14). A receivable J2 does not hold is refused with `ReceivableNotFound`.
- **`refund_deposit`** refunds one deposit, with no product involved (K2 invariant 5). If a refund of the deposit is in progress, it does nothing. Otherwise it runs steps 1 to 3 below. A deposit whose cheque carried no return receivable is not refunded: the refund is recorded as failed (invariant 3).
- **`run_due`** posts every refund cheque recorded and not yet posted, whose claim deadline has not passed, and marks each posted in J2 once M1's `post` succeeds. It returns none.

`refund_deposit` runs these steps:

1. Record the refund (J2 `record_refund`), with its key: the count of the deposit's refunds that have ended (J2 `ended`) gives the key's input (invariant 2, section 1). Then `export` the deposit's refundable value from the receivable's purse, or from the purse that purse was drained into (J2 invariant 4), with transactions rebuilt until the refund cheque's own claim deadline: the payee's send time plus the payee's claim timeout.
2. Seal the refund cheque to the deposit's return receivable, with no return receivable of its own (H3 invariant 5), with a fresh ephemeral key and nonce from section 1's randomness. Commit the export in the store transaction that records the sealed cheque as an outgoing cheque (J2 `record_outgoing`).
3. Post the refund cheque to that return receivable's refund topic (H2, M1), signed with the statement-store key of the product the refund belongs to: the product that asked, or, for an automatic refund, the product that made the deposit (J2 `DepositRecord.product`). If the post fails, `run_due` posts it again.

**Invariants**
1. Each deposit is refunded to the return receivable it came with, and never with more than the deposit cleared.
2. A refund is keyed by its deposit and by the number of earlier refunds of that deposit that have ended (J2 `ended`). So a retried refund joins the refund in progress and never pays twice. A refund requested after the previous one ended is a new operation.
3. A deposit whose cheque carried no return receivable cannot be refunded. The refund fails for that deposit (`Internal`, H5). This is RFC-17's case of no usable return path.
4. A refund cheque is recorded as an outgoing cheque (J2), so K5 handles it like any other.
5. **A refund is `Done` only when the payer has claimed it**, that is, when its coins' export status is `Claimed` and stable (layer 1, B1). When the refund's export completes, that only means the coins were minted.

---

#### K4 · Cancel

**Owns** nothing; its records are J2's. **Band:** pure decision with an effectful half.

```rust
fn cancel(cheque: ChequeHash) -> Result<Stream<CoinPaymentStatus>, CoinPaymentError>;
```

- **`cancel`** reclaims the coins of one of the payer's own cheques that are still unclaimed (layer 1, `reclaim`), into the cheque's source purse, or into the purse that purse was drained into (layer 1, B2). If a reclaim of the cheque is in progress, including K5's, `cancel` joins it (invariant 3). Otherwise it records a new reclaim (J2 `record_reclaim`) and starts it. It returns the status H5 invariant 4 derives from the cheque's progress. A hash J2 does not hold as an outgoing cheque is refused with `ReceivableNotFound`, as `cheque_status` does (K1).

**Invariants**
1. Only the payer's own outgoing cheques, as recorded in J2, can be cancelled.
2. **A cancellation does not wait for the claim deadline**, so the payer and the payee may both try to move the same coin. The payer chooses this explicitly. Whoever moves a coin first has it (layer 1, C1 invariant 4).
3. **A reclaim is keyed by the cheque's hash and by the number of earlier reclaims of that cheque that have ended** (J2 `ended`). So a cancellation and K5's reclaim of the same cheque join one operation, instead of competing. A coin that an ended reclaim left in place is reclaimed by the next reclaim.

---

#### K5 · Outgoing cheques

**Owns** nothing. It schedules work for J2's outgoing cheques. **Band:** pure decision with an effectful half.

```rust
fn run_due(now: Time) -> Option<Time>;
```

- **`run_due`** does two things for each outgoing cheque J2 holds, payments and refunds alike, and returns the earliest time at which either next comes due:
  - **Reclaim** whatever is still unclaimed (layer 1 `reclaim`, into the source purse or the purse that purse was drained into). The first reclaim starts at the claim deadline plus the protocol's maximum claim validity window plus the maximum clock tolerance (RFC deviations, gap 5). It is recorded in J2 (`record_reclaim`) and uses the same key as a cancellation (K4 invariant 3), so the two join. After that, whenever no reclaim of the cheque is in progress and some coin's export status is not yet stably `Claimed`, `Reclaimed` or `NeverMinted` (layer 1, B1), `run_due` starts the next reclaim. It stops when every coin has one of those stable statuses. The validity window covers a claim submitted just before the deadline, which may still land. The tolerance covers a payee whose clock runs a little behind.
  - **Collect refunds** for a payment cheque, until its claim deadline plus the refund window. On each call it reads everything retained on the return receivable's refund topic (M1 `fetch`), and while the layers are running it also subscribes to the topic (M1 `subscribe`). Each refund cheque found is deposited as K2's `deposit` does, into the source purse. A refund is never refunded (H3 invariant 5).

**Invariants**
1. K5 needs no product. It collects a refund, and reclaims a cheque nobody claimed, even if the product that paid no longer exists.
2. K5 stops collecting refunds at the refund deadline.
3. K5 runs only when it is invoked: through the one entry point (section 1), or on a new block while the layers are running.

---

### Band N — Surface
*Thin: validation, authorization and translation.*

---

#### N1 · RFC-17 surface

**Owns**, in memory, the access that authorized each open stream of N1 and N2, so that a revocation closes the right streams (invariant 5). **Band:** thin surface.

N1's operations are RFC-17's calls, with the changes `rfc-deviations.md` records. Each call is served as the table says, after J1 has authorized the action in the last column on the purse named there. J1's `Ask` goes to the user through the consent interface (section 1); a refusal, by J1 or by the user, is `Denied`. The calls `cancel_cheque`, `cheque_status` and `list_purses` are new methods of the `CoinPayment` wire trait, added after the shipped ones. N1 calls K1 `pay` with `Initiator::Product`. When `pay` returns `NeedsConfirmation`, N1 asks the user through the consent interface whether to spend value that is still gaining privacy, and calls `pay` again with `confirmed` if the user agrees; otherwise it returns `Denied`.

| RFC-17 call | Served by | J1 action, on |
|---|---|---|
| `create_purse` | Layer 1, with the creator as the purse's tag (J1) | none beyond the `CoinPayment` permission (invariant 1) |
| `list_purses` (RFC deviations, gap 8) | Layer 1, filtered through J1 | query, each purse |
| `query_purse` | Layer 1 | query |
| `rebalance_purse` | Layer 1 | spend, the source purse; receive, the destination |
| `delete_purse` | Layer 1, which records the purse it drained into (layer 1, B2) | the purse's creator only, with spend on it; receive, the drain target |
| `create_receivable` | H1, recorded in J2 | receive |
| `create_cheque`, returning the cheque at once, as shipped (RFC deviations, entry 3) | K1 `pay` | spend |
| `listen_for_payment` | K2 `listen` | receive, the receivable's purse |
| `deposit` | K2 `deposit` | receive, the receivable's purse |
| `refund` | K3 `refund` | spend, the receivable's purse |
| `cancel_cheque` (RFC deviations, gap 7) | K4 `cancel` | spend, the cheque's source purse |
| `cheque_status(cheque)` (RFC deviations, entry 3) | K1 `cheque_status` of the cheque's hash, also after a reload | query, the cheque's source purse |

**Invariants**
1. Every call is authorized through J1 before anything else happens, the `CoinPayment` permission first (J1 invariant 5).
2. A product never sees a coin secret, except sealed inside a cheque. It also never sees a derivation path, a coin's identity or a detail of a recycler. The leaves of a clearing reference are the one intended exception (H4 invariant 2).
3. `deposit` needs receive access to the receivable's purse, as `create_receivable` does. So a product that holds someone else's cheque cannot learn the outcome of depositing it. A caller that may not query a receivable's purse gets the same answer as for an unknown receivable, `ReceivableNotFound`, without the user being asked, so the caller cannot tell which other products the user has.
4. `list_purses` names only purses the product may query. Another product's purse stays invisible unless the user granted access, because listing it would reveal which other products the user has.
5. **Access is checked on the purse that the value leaves or lands in**, which is the purse the table's last column names. For a deleted purse, the check applies to the purse it was drained into (layer 1, B2). When the user revokes a permission in the permission service (J1), N1 closes every open stream that this permission authorized, on this surface and on N2.

---

#### N2 · RFC-6 surface

**Owns** nothing. **Band:** thin surface.

N2's operations are RFC-6's calls, served as the table says after J1's authorization. N2 is the only implementation of RFC-6: the core serves it from layer 1, and the host supplies no payment engine of its own. N2 calls layer 1 `offload` with `Initiator::Product`. When it fails with `NeedsConfirmation`, N2 asks the user through the consent interface whether to spend value that is still gaining privacy, and calls it again under the same key with `confirmed` if the user agrees; otherwise it returns `Rejected`. An id that the calling product has not used is answered as unknown: `PaymentNotFound` for `status_subscribe`, `TopUpNotFound` for `top_up_status_subscribe`. This holds for another product's id too, since that id names nothing in the caller's scope (invariant 3). A status subscription reports each mapped status once, and a status layer 1 cannot read ends it with `Unknown`, never with an outcome (layer 1, P11).

| RFC-6 call | Served by | J1 action |
|---|---|---|
| `balance_subscribe(purse?)` | Layer 1 `balance`; `available` is the spendable bucket | query, with RFC-6's balance access |
| `top_up(into?, amount, source, id)` | Layer 1 `top_up` for a product account or a private key (a 64-byte sr25519 secret); layer 1 `import` for coins (RFC deviations, entry 12). Keyed by `id` | top up |
| `top_up_status_subscribe(id)` | Layer 1 `status`, through J2, mapped as H5 says | the top-up's own product only |
| `request(from?, amount, destination, id)` | Layer 1 `offload`, keyed by `id` | spend |
| `status_subscribe(id)` | Layer 1 `status`, through J2 | the payment's own product only |

**Invariants**
1. A call that names no purse acts on the main purse.
2. An RFC-6 call is authorized exactly as an RFC-17 call on the same purse, except for the `CoinPayment` permission of J1 invariant 5 (RFC-17, behavioural requirement 17), and except that reading the main purse needs RFC-6's balance access instead of a purse permission (J1 invariant 6).
3. **The id the product chose is the key of the operation.** The key of a payment or top-up is that id hashed together with the product, as the core runtime passes the product on. The key lives in the product's own layer-1 scope and is recorded before the operation starts (section 1). A call whose id already names an operation is refused (`AlreadyExists`), before the user is asked to confirm anything, because the refused call could spend nothing. So a product that lost the response can retry safely: the retry is refused without a second prompt, and the product follows the first operation through its status. The record written before the operation starts still refuses an id that two calls race for. Another product cannot name the operation at all.
4. A top-up from coins (RFC-0021) is an import: the product hands coin secrets in and never receives any. A top-up from a product account or a private key does not fail while the source lacks the funds. It waits for them (`Detecting`) until the top-up window passes (section 1). Only one top-up per source runs at a time; another one is refused (`SourceBusy`). For coins, the source is busy while a claim without a stable verdict holds one of them (layer 1 `importing`), so a top-up that shares even one coin with a running import is refused before anything is recorded. This matches layer 1, which builds one transaction per signing account at a time (layer 1, C2 invariant 5). A private key must be a 64-byte sr25519 secret in the canonical layout, read the same way layer 1 reads a coin secret (layer 1, G1 invariant 5). A key in any other layout is refused (`InvalidSource`), and so is a coin secret that layer 1's `import` refuses as not canonical (layer 1, G1 `InvalidSecret`).
5. **Amounts are in the payment asset's own units** (RFC-6). Coinage counts whole cents, so the balance is converted from cents into the asset's units with the instance's `asset_unit` (layer 1, A3). A top-up or a payment moves its amount rounded down to a whole cent. If it moves less than was requested, it ends `ClaimedPartially`. A zero amount is refused (`InvalidAmount`).
6. **A funding session's top-ups and payments are N2's own.** A funding provider's worker may start a top-up or a payment for a funding session it serves (RFC-6 version 2's `funding` field). N2 records the session with the operation (J2 `Rfc6Record.funding`) in the store transaction that records the operation, and the core's funding registry, which tracks what each session moved, asks N2 whether it holds an operation under an id, and what became of it, instead of a host engine. An id that names an operation of another session, or one without a session, is `AlreadyExists`, as for any reused id.

---

#### N3 · Invoice payment

*Deferred: not part of the first release.* Paying an invoice with no product involved needs a key to post the cheque with that belongs to no product (M1), which is not decided. Until then, a product pays an invoice through N1.

**Owns** nothing. **Band:** thin surface.

A user agent can pay an invoice with no product involved: it reads the invoice, asks the user, pays through K1, and posts the cheque to the invoice's payment topic.

```rust
struct Invoice { version: u8, to: Receivable, amount: u32 }   // SCALE-encoded; version 1; amount in cents, like Cheque.amount

enum InvoiceError {
    NotAnInvoice,      // not the text form below, or the bytes do not decode
    UnknownVersion,    // invariant 2
    BadReceivable,     // a `to` that H3 refuses to seal to (invariant 2)
}

fn parse_invoice(text: String) -> Result<Invoice, InvoiceError>;
fn pay_invoice(invoice: Invoice, source: PurseId)
    -> Result<Stream<CoinPaymentChequeProgress>, CoinPaymentError>;
```

- **`parse_invoice`** reads the text form below and checks the version and `to` (invariant 2). It asks nobody.
- **`pay_invoice`** asks the user, through the consent interface (section 1), to confirm the amount, the payee and the source purse (invariant 1), and warns the user if J2 shows an earlier payment to the same `to` (invariant 3). If the user refuses, it returns `Denied` and nothing is exported. If the user confirms, it calls K1 `pay` with `Delivery::Post` and `Initiator::User`, and returns K1's `cheque_status` for the cheque. When `pay` returns `NeedsConfirmation`, it asks the user again, now whether to spend value that is still gaining privacy, and calls `pay` again with `confirmed` if the user agrees; otherwise it returns `Denied`.

The encoding is standard, so any user agent can read any product's invoice. The text form of an invoice is `coinage-invoice:` followed by the unpadded base64url encoding of those bytes. A QR code holds this text unchanged, and a deep link carries it as its `invoice` query parameter. An invoice needs no channel field, because the payment topic is computed from `to` (H2).

**Invariants**
1. The user confirms the amount, the payee and the purse it is paid from before anything is exported.
2. `version` allows a later version of the invoice to name another kind of channel. An invoice with any other version, or with a `to` that H3 would refuse to seal to, is rejected before the user is asked.
3. If this user agent has already paid an invoice, which it tells from J2's outgoing cheques, paying it again asks the user again. Invoices are QR codes, and it is easy to scan one twice.

---

## 4. Open decisions

- **The format of `HopReference`** (M1), the reference by which the channel points to a cheque too large for a statement. Neither RFC-17 nor this design defines it yet.

- **The key N3 posts with** (M1). Every post layer 2 makes belongs to a product and is signed with that product's statement-store key. Paying an invoice with no product involved (N3) needs a key that belongs to no product, which is not decided, so N3 is deferred.

- **A share of the unload-token allowance per purse**, postponed. It would remove Q5's residual about the shared token allowance. The cost is that one purse could hold tokens it does not use while another purse has none.

Layer 1's open questions are listed under "What this does not decide" in its own document.

## 5. Composition: why the payment properties hold

Section 3 states what each component guarantees. This section argues that these components together, on top of layer 1, give payments the properties below. Each argument lists the ways the property could fail and names what excludes each one. If a way to fail is missing from a list, the argument has a gap, whatever the cited invariants say.

Layer 1's own properties are assumed here; layer 1's composition section argues for them. Where a layer 1 property applies unchanged, the row says so instead of repeating the argument.

---

### Q1 · No value is lost between payer and payee

*Value that a payer puts into a cheque ends up either with the payee or back in the payer's purse, never with nobody.*

| How it could fail | Excluded by |
|---|---|
| The cheque never reaches the payee: it is lost in transport, or removed from the channel before the payee read it | K5: the payer reclaims it after the claim deadline |
| The payee never deposits it | K5, likewise |
| The payer's user agent crashes after exporting, before the handoff is committed | Layer 1, G1 invariant 3: the coins of an uncommitted export return to the purse on restart |
| The payer's user agent crashes after the commit, before it hands the cheque over | K1 invariant 1: the cheque was recorded in the same store transaction as the commit, so K5 reclaims it |
| Nobody can open the cheque, because the payee's receivable secret is lost | The coins never left the payer's accounts; K5 reclaims them |
| The coins are never minted | Layer 1, E3 invariant 4: the inputs are free again (layer 1, B1 invariant 2) |
| The payee claims while the payer cancels | K4 invariant 2: the chain moves each coin once, and whoever moves it first has it |
| The payee claims just before the deadline while the payer reclaims | K5: the reclaim waits until one validity window and the clock tolerance have passed after the deadline. By then, any claim submitted before the deadline has either been included or can no longer be included (K2 invariant 3). Both clocks keep network time |
| Payer and payee compute different deadlines, because their claim timeouts differ | H3 invariant 6: the cheque carries one deadline, which both use |
| A refund cheque is never posted, because the channel is unreachable or the user agent crashed before posting | K3 `run_due` posts it again from the sealed cheque J2 keeps (J2 `unposted`), until its claim deadline; after that, K5 reclaims it |
| A reclaim ends with a coin left in place, for example because a lock held it | K5 `run_due` starts the next reclaim, under a new key (K4 invariant 3), until every coin of the cheque is stably `Claimed`, `Reclaimed` or `NeverMinted` (layer 1, B1) |
| A reclaim lands in a purse that has been deleted since | K4, K5: it lands in the purse the deleted purse resolves to, which is always open (layer 1, B2 `resolve_purse`, B2 invariant 7) |
| A refund arrives after the payer stopped collecting refunds | K5: the payee's K5 reclaims the refund cheque at its deadline, and the payee can refund its value again (J2 invariant 1) |
| A payment or refund arrives for a receivable whose purse was deleted | J2 invariant 4: it lands in the purse the deleted purse resolves to (layer 1, B2 `resolve_purse`) |
| The deposit lands in a purse other than the receivable's | K2 invariant 2, J2 invariant 5 |
| A deposit clears only part of a payment, and the payee keeps a part nobody agreed to | K2 invariants 4–5: the deposit fails, and the value that cleared is refunded automatically |
| A runtime upgrade fails layer 1's check while a cheque is in flight, so nothing can be built | Layer 1, G3 invariant 7: no record is given up because of the check, and each record's own deadline still applies. A minting that never ran frees its inputs, and once a runtime passes the check, K5's reclaim takes back whatever the payee did not claim |
| Layer 2's records are lost, but layer 1's are not, so committed exports are never reclaimed | Section 1: layer 2's records are kept in layer 1's store, so they are lost only together with layer 1's records |
| The payer's whole store is wiped while a cheque is in flight | Layer 1's reconstruction adopts the exported coins again and counts them toward the main purse, unless the payee claims them first (layer 1, P6). For a product purse that no backup lists, this happens only once its identifiers are walked (layer 1, P1 residual) |
| The payee's store is wiped after depositing | Layer 1's reconstruction finds the coins and counts them toward the main purse, with the same limit for a product purse that no backup lists |

**One residual, inherited.** After a wipe on the payer's side, the payer's balance counts coins that the payee may still claim. This is layer 1's P6 residual. No value is lost in either case: whichever side moves a coin first has it, and if the payer spends it first, the payee's deposit fails.

---

### Q2 · Nothing is paid or claimed twice by accident

*No retry, repeated delivery or rescan makes a payer pay more, or a payee claim more, than was intended.*

| How it could fail | Excluded by |
|---|---|
| A cheque is delivered twice, or deposited twice, for example after `run_due` posts it again | K2 invariant 1: a deposit is keyed by the cheque's hash (H3 `cheque_hash`) |
| A crash between recording a deposit and starting its import | Section 1: the record names the key of the import, so after the restart that one import is either joined or started |
| A product retries `create_cheque` after the user agent crashed | K1 invariant 5: the first cheque was never handed over, so it pays nobody, and K5 reclaims it |
| A product passes a cheque on before storing it, crashes, and retries `create_cheque` | **Not excluded.** The call takes no identifier (K1 invariant 5) |
| The user pays the same invoice twice | N3 invariant 3, once N3 is built: the second payment asks the user again. Until then a product pays invoices, and each payment is confirmed by the user (J1 invariant 4) |
| A refund is retried | K3 invariant 2: the refund is keyed by its deposit and the number of its ended refunds (J2 `ended`), so the retry joins it |
| A deposit and a reclaim of the same cheque share a key, in a payment between two products of one user agent | Section 1: each key is hashed under a label that names its kind of operation (`coinage/deposit`, `coinage/reclaim`) |
| The payer's K5 deposits the same refund twice | K2 invariant 1, as for any cheque |
| The payer cancels a cheque while K5 reclaims it | K4 invariant 3: both are keyed by the cheque's hash and the number of its ended reclaims, so they join one operation |
| A payer puts the same coins into two cheques | Layer 1, C1 invariant 4: an exported coin is never exported again; a dishonest payer's second cheque fails as sniped |
| A product retries an RFC-6 `request` or `top_up` after losing the response | N2 invariant 3: the id it chose names the first operation, so the retry is refused (J2 `record_rfc6`) and the first operation's status answers it |

**One residual**: `create_cheque` takes no identifier. So a product that passed a cheque on, then lost it and retries, makes a second payment.

---

### Q3 · A refund never exceeds what arrived

*For each deposit, a payee returns at most what that deposit cleared, and returns it to the payer who made that payment.*

| How it could fail | Excluded by |
|---|---|
| A refund is issued twice | K3 invariant 2: a refund of a deposit in progress is joined, never started again |
| A failed deposit's own refund and a product's `refund` both run | K3 invariant 2: both are keyed by the deposit, so the product's call joins the refund already running |
| A refund is issued before the deposit has settled | J2 invariant 1 (J2 `refundable`): only cleared value with a stable outcome counts |
| A refund that came back, because the payee reclaimed it, blocks every later refund | J2 invariant 1: a reclaimed refund counts as nothing; K3 invariant 2: the next refund gets a new key |
| A refund that came back is counted as paid, and a new refund overpays | J2 invariant 1 likewise: a refund counts only once the payer has claimed it |
| Several payers paid one invoice, and one payer gets the others' money | K3 invariant 1: each deposit is refunded to its own return receivable |
| The purse has spent the value since | The refund's export fails for insufficient funds, and the refusal is recorded (J2 `record_refusal`); no partial refund is paid |
| A refund is reported done while the payer does not have it | K3 invariant 5 |

**One residual, inherited** from layer 1's P6: after a wipe, a refund may be paid with a coin that reconstruction adopted again although its secret had been handed out. That refund counts as claimed when the earlier payee takes the coin.

---

### Q4 · No product reaches another's purse

*A product queries, receives into or spends from a purse only as the user allows.*

| How it could fail | Excluded by |
|---|---|
| A product names a purse it did not create; purse identifiers are linear and easy to guess | J1 invariant 1, N1 invariant 1: an identifier gives no access by itself; every call is authorized |
| A product takes the RFC-6 path around RFC-17's checks | N2 invariant 2 |
| A product names another product's payment | N2 invariant 3: payment ids live in the product's own scope, so another product's id is answered as unknown (`PaymentNotFound`, `TopUpNotFound`) |
| A product deposits a cheque meant for someone else's receivable, to learn how it went | N1 invariant 3 |
| A product reaches a purse through a deleted purse's identifier | N1 invariant 5: access is checked on the purse the deleted purse resolves to (layer 1, B2 `resolve_purse`) |
| A product presents a purse identifier from before a reinstall | Layer 1, B2 invariant 5, and J1 invariant 1: the purse comes back only from the backup, with its creator and with no value; without a backup, the identifier names no purse of that product (RFC deviations, gap 6) |
| A product lists purses to discover another product's purses | N1 invariant 4 |
| A purse identifier reused after a reinstall hands a new product an old product's value | Layer 1, B2 invariant 5: a purse holds only the current installation's coins |
| A product reaches the main purse | J1 invariant 1: no product has access to it by default |
| A product the user never approved creates or reaches a purse | J1 invariant 5: the `CoinPayment` permission comes first |
| A product keeps using an access the user has revoked, through a stream it opened before | N1 invariant 5: when the user revokes a permission, every open stream that permission authorized is closed, on N1 and on N2 |

**No residual.** Which access the user grants, and when the user is asked, is a policy choice (J1 invariant 4). The property is only that no product reaches a purse without passing that policy.

---

### Q5 · A product learns no more than it is entitled to

*Nothing that layer 2 hands to a product reveals a coin secret, a derivation path, a coin's identity or anyone else's activity, except what a clearing reference must reveal.*

| How it could fail | Excluded by |
|---|---|
| `create_cheque` hands the product coin secrets | N1 invariant 2, with H3 invariant 1: the secrets are sealed to the payee |
| A receivable on a low-order point makes its cheques readable by anyone | H3 invariant 1: such a receivable is refused |
| A product holding a cheque learns the payer's return receivable, and with it the refund topic | H3 invariant 4: the return receivable is inside the seal |
| A topic reveals a purse, a product or an amount | H2 invariant 2 |
| Someone who saw the invoice watches for its refund | H3 invariant 4: the refund topic comes from the return receivable, which is inside the seal |
| A refusal tells a product the balance it may not see | H5 invariant 2 |
| A receivable used twice links two payments | H1 invariant 1: every receivable is a fresh draw; K1 step 3 draws a new return receivable for each payment, and N1's `create_receivable` a new receivable for each call |
| A product lists purses and learns which other products the user has | N1 invariant 4 |
| A product asks for the progress of a cheque it did not create | N1: `cheque_status` needs query access to the cheque's source purse; K1 `cheque_status` answers `ReceivableNotFound` for a hash J2 does not hold as an outgoing cheque |
| A product probes another product's payment ids to learn which exist | N2: an id the caller has not used is answered as unknown, also when another product used it (N2 invariant 3) |
| A clearing reference names the payer's coins | **Not excluded.** H4 invariant 2: naming them is what makes the reference verifiable |
| Someone who saw an invoice learns when it was paid | **Not excluded.** The payment topic is computed from the invoice, so they can see a cheque arrive, but not what it holds |
| A product watches its spendable figure, or a `NoUnloadToken`, change as other purses spend unload tokens | **Not excluded.** The token allowance belongs to the credential and is shared by all purses (layer 1, B2) |
| A product that knows a coin's secret deposits or tops up that coin, to learn whether this user agent already holds or claimed it | **Not excluded.** Layer 1 refuses at once to claim a coin that is already an input of one of its own transactions (layer 1, C1 invariant 2). A user agent that does not hold the coin would instead wait and report the coin missing |
| The statements a user agent posts are signed by one account, so an observer of the statement store links its payments and refunds to each other | **Not excluded.** Which account signs layer 2's statements is an open decision (section 4) |

**Five residuals.** The first, about clearing references, is intended. A verifiable clearing reference names transactions. From those transactions a receiving product learns where the payer's coins came from, the payer's change if a split was made, and the accounts of its own purse. A refund's clearing reference names the payer's fresh account directly. H4 invariant 3 lets the product show a third party one leaf at a time (RFC deviations, gap 9). The second, about invoice readers, applies to any invoice that says where to deliver: whoever reads the invoice can watch for the delivery. The third follows from one token allowance being shared by all purses; a fixed share per purse would remove it. The fourth follows from refusing a double claim at once: a product that already knows a coin's secret learns one bit about that coin. The fifth is open: until section 4 decides which account signs layer 2's statements, nothing prevents that account from linking a user agent's statements to each other.

---

### Q6 · No payment is reported as more settled than it is

*Neither RFC surface reports a payment, a deposit or a refund as further along than layer 1's verdicts support.*

| How it could fail | Excluded by |
|---|---|
| Receiving a cheque is reported as being paid | K2: receiving a cheque is not being paid (RFC-17) |
| The return of `create_cheque` is read as the payment having happened | K1 `cheque_status` (N1) reports minting, claimable, claimed and returned value separately, and counts a claim only once it is stable (H5 invariant 3) |
| A cheque that does not open, or whose coins do not add up to its amount, is reported as a payment in progress | K2 `deposit`: such a cheque is refused with `BadCoins` before anything is recorded or claimed |
| A deposit is reported done before it is stable | K2 invariant 4, H5 invariant 1 |
| A refund is reported done when its coins are only minted | K3 invariant 5 |
| A refund or cancellation is reported failed while some of it can still be claimed | H5 invariant 4: `Failed` only once nothing is minting or claimable |
| A reclaim is counted as returned before it is stable | H5: `Reclaimed` counts as returned only once it is stable |
| A deposit is reported done though its coins were worth less than the cheque said | K2 invariant 4: `Done` only at the cheque's amount; anything else is `Failed` and refunded (K2 invariant 5) |
| A clearing reference rests on a provisional outcome | H4 invariant 1 |
| A provisional success is shown, then withdrawn by a reorganisation | H5: every state before a stable verdict maps to `Clearing`, so RFC-17 never shows a provisional success that could be withdrawn |

**One residual, inherited** from layer 1's P6: after a wipe, a cheque may be paid with a coin that reconstruction adopted again although its secret had been handed out. That cheque reports as claimed when the earlier payee takes the coin.

---

### Q7 · Every flow ends

*No deposit, outgoing cheque or refund stays open indefinitely.*

| How it could fail | Excluded by |
|---|---|
| A deposit waits forever for coins that never come | K2 invariant 3: the deposit stops at the claim deadline |
| An outgoing cheque is never claimed | K5: the cheque is reclaimed after the deadline |
| A reclaim is repeated forever | K5 `run_due`: it stops once every coin of the cheque is stably `Claimed`, `Reclaimed` or `NeverMinted` (layer 1, B1). A coin is left in place only while a lock holds it, and lock periods are finite (layer 1, A3) |
| The payer waits forever for a refund | K5 invariant 2: K5 stops collecting at the refund deadline |
| A refund waits forever for the payer to claim it | K3 invariant 4: the payee's K5 reclaims the refund cheque at its deadline |
| A refund and the refund of that refund go back and forth between payer and payee | H3 invariant 5: a refund cheque carries no return receivable, and K2 `run_due` never refunds a deposit received under a return receivable (J2) |
| A failed deposit's automatic refund is never started, for example after a crash | K2 `run_due`: it starts the refund of every failed deposit whose automatic refund J2 does not record as made (K2 invariant 5) |
| A payment waits forever for inputs that never become usable | K5: at the deadline it reclaims the coins, giving up any minting that has no attempt in flight (layer 1, C2 invariant 5) |
| A runtime upgrade fails layer 1's check, and no runtime that passes it follows | **Not excluded.** Layer 1, G3 invariant 7: records wait while the check fails. Deadlines still end deposits and exports, but a reclaim cannot be built until a runtime passes the check |
| Nothing ever invokes layer 2 | **Not excluded.** Section 1: the host must call the one entry point, as in layer 1 |

**Two residuals, both inherited.** A deadline has an effect only when the layers run. On a host that never invokes the layers, a stranded cheque is never reclaimed. This is layer 1's P1 and P8 residual, reached through layer 2. And while a runtime fails layer 1's check, no transaction is built, so a cheque that needs reclaiming waits until a runtime passes the check (layer 1, G3 invariant 7).

---

### What this section does not establish

1. **It is an argument, not a proof.** It is written so that naming a way to fail that it missed shows it to be incomplete.
2. **Two residuals are significant.** The clearing reference reveals the payer's coins and the product's own accounts (Q5), because that is what makes it verifiable. Every deadline depends on the host invoking the layers (Q7). The other residuals are smaller: an invoice's reader can see when it was paid (Q5); a payer's balance is overstated after a wipe (Q1), which is layer 1's residual; a cheque or refund paid with a coin that reconstruction adopted again can report as claimed when the earlier payee takes that coin (Q3, Q6); a product that retries `create_cheque` after losing a cheque it had passed on pays twice (Q2); the shared token allowance shows a product the unloads of other purses (Q5); a product that knows a coin's secret learns whether this user agent already claimed it (Q5); until section 4 decides which account signs layer 2's statements, that account may link a user agent's statements (Q5); and a reclaim waits while a runtime fails layer 1's check (Q7), which is layer 1's.
3. **Layer 1's properties are assumed.** Every row that cites layer 1 relies on layer 1's argument for it.

## 6. References

Names in square brackets refer to this list. Layer 1's document lists the references it relies on in its own section 8; this list adds those that layer 2 uses. Code follows the notation defined in layer 1's section 2.

- [binary-merkle-tree] Polkadot SDK's `binary-merkle-tree` library, version 16.1.2, functions `merkle_root` and `merkle_proof`: https://crates.io/crates/binary-merkle-tree/16.1.2
- [RFC 7693] The BLAKE2 cryptographic hash: https://www.rfc-editor.org/rfc/rfc7693
- [RFC 7748] Elliptic curves for security, X25519: https://www.rfc-editor.org/rfc/rfc7748
- [RFC 8439] ChaCha20 and Poly1305 for IETF protocols: https://www.rfc-editor.org/rfc/rfc8439
- [RFC-6], [RFC-17], [RFC-0008], [RFC-0021] The TrUAPI RFCs with those numbers, in https://github.com/paritytech/host-rust-core under `docs/rfcs/`: `0006-payments.md`, `0017-coinage-payment.md`, `0008-statement-store.md` and `0021-payment-topup-coins.md`
