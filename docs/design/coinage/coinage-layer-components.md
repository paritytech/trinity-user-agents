---
title: "Coinage Layer — component model"
status: "Proposal"
---

# Coinage Layer — component model

This document specifies the coinage layer as 22 components. For each component it states what the component owns, the interface it presents and the rules it must keep. Each component can be reviewed on its own.

§7 argues that the components together give the system the properties it needs. This does not follow from each component being correct on its own, so §7 makes the argument separately, and it can be checked separately.

Throughout, **the layer** means the coinage layer. The durability layer, which submits the coinage layer's transactions and judges their outcome, is always named in full.

## 1. What the layer does not own

The layer uses seven things that it does not hold itself. Each is supplied from outside:

| Interface | What it supplies |
|---|---|
| **Chain** | State reads at a named block |
| **Durability layer** | A record per transaction, its submission and its status, and background execution while any record is open (below) |
| **Store** | The [shared store](store-contract.md): transactions that span the layer, the durability layer and the layers above, on a store that is never restored |
| **Clock** | The current time, as an argument wherever time matters |
| **Root entropy** | The seed every key derives from |
| **Randomness** | One draw: a new installation identifier |
| **Privacy policy** | The thresholds that set how much availability is given up for privacy (A4) |

Given the same seven inputs, the layer is deterministic: two implementations with the same seed, against the same chain, produce the same on-chain effects and the same records. This is what makes conformance between implementations testable.

An installation identifier must be unique across every reinstallation on the same seed. If one repeated, the new installation would derive the same keys as an earlier one, which is exactly what installation identifiers exist to prevent.

**The store carries one obligation beyond atomicity: it is never restored.** A host must exclude the store from every backup and device-transfer mechanism it offers, so that the seed is the only thing that survives a wipe. The layer cannot enforce this. It cannot detect a violation either: a restored store is byte-identical to one the layer wrote itself, so no marker inside the store can tell the two apart. Instead, the layer is designed to work correctly when the store is missing: a fresh store means a fresh derivation namespace (G3), and everything the layer needs to find what earlier namespaces hold is published on chain rather than kept locally (F4). A host that restores the store anyway makes the layer re-derive identifiers it already handed to payees.

**The purse metadata is the one exception.** Each purse's name, creation time and tag may be backed up and restored. None of these records which keys have been used, so restoring them makes the layer re-derive nothing. The backup is how a reinstall recovers which caller owns which purse identifier. It does not bring back the purse's earlier coins, which count toward the main purse (B2 invariant 5). The backup is best-effort: when it is missing, or older than a purse, only that purse's owner is lost; the assets are recovered either way (F4).

### The chain

The chain is the coinage pallet [coinage pallet] on the People chain and, for the installation registry, the AccountDataStore contract on Asset Hub (F4 invariant 4). The layer reads chain state at a named block (F1) and submits transactions only through the durability layer. It relies on the following facts about the chain. A3 checks the values these facts depend on.

- **Instances.** The chain runs several coinage instances, each wrapping one external asset at one unit. The layer serves exactly one instance, named by its configuration (A3). A coin of another instance does not belong to this layer.
- **Calls** the layer submits: split a coin; transfer a coin; recycle a coin into a recycler entry; unload entries into coins; unload entries into an external asset; load entries from an external asset; and, for the installation registry, the contract call that appends a record.
- **Origins** are the ways a transaction proves that it may act: as a coin, as an unload token (feeless, D1), as an unpaid signed load, or, for the registry, as the `//datastore` account with a PGAS claim that proves personhood [RFC-0010].
- **Rings.** A ring is the anonymity set a recycler entry belongs to. It is addressed by index and revision, and a proof built against one revision does not verify against another. A ring is paged, and a member's position is reported within its page (B1 invariant 4).
- **Coin age.** A coin has an age, stored on chain. A transfer or a split gives each output the input's age plus one. An unload into coins creates its coins at age 1, so a coin the layer unloads already counts one step towards the limit. Past the chain's maximum coin age a coin can no longer be transferred or split, but it can still be recycled. The layer stops spending a coin earlier, at the age cutoff (A3).
- **A failed dispatch costs different things for different origins.** With a coin origin, the coin is returned and locked for a time; once the lock has expired, the same bytes can be included again while their window is still open. With a token origin, the token is lost and nothing is restored: the chain records the token as consumed when the attempt is included, whether or not its dispatch succeeds. An unpaid signed load never fails its dispatch in a valid block, because the runtime turns such a failure into an invalid transaction. Every component that decides about retries uses these facts (C2 invariants 2 and 3, D1).
- **What an effect leaves on chain.** A successful transaction consumes all its inputs and creates all its outputs in one step. A successful unload marks each alias it presents as unloaded. A failed unload paid with a free token, the only kind this layer submits (D1), leaves the aliases as they were; a failed unload that pays its fee from its own output changes them, which is one reason the layer does not use that origin. The chain does not check an unload's alias proofs or alias states when it validates the transaction, only when it dispatches it, so the layer checks that every alias it presents is not yet unloaded and not locked before it builds an unload (C2). Cleanup removes some of this again: a ring's entries and their alias states when the ring is cleaned up, and a token's consumption once its period is no longer accepted. So the absence of an entry, an alias state or a token's consumption proves nothing by itself (C2 invariant 2).

### The durability layer

The durability layer is a separate component, shared with every other domain that submits transactions: the durability engine of the TrUAPI core runtime [durability engine]. Its internals (submission, following an attempt, recovery passes, the search through finalized blocks, reorganisation handling) belong to it and are not specified here. In the durability layer's own terms, a record is a row of its ledger, identified by a `DurableTxId`; an operation is a group; registration is `execute_with` with a `DurableRequest::deferred_in`; a record's policy parameters are its `policy_params`; *waiting to be built* is `PendingSubmission`; *given up* is the failure `Abandoned`; a completion oracle is a `CompletionOracle` with its `PassScope`; and a rebuild policy is a `SubmissionPolicy`. This section specifies only the boundary: what the coinage layer relies on, and what it supplies.

**Domains.** The durability layer serves named domains, and each domain has exactly one chain, one completion oracle and one rebuild policy. The layer is two domains: `coinage`, on the People chain, which runs the coinage pallet, for every transaction except the installation registry's append; and `coinage-registry`, on Asset Hub, for that append (F4 invariant 4). C2 defines both.

**How the core wires them.** The core creates the durability layer when the host starts the core's services (`start_core_services`). It first builds the list of domains, a `DurableRegistry`, by passing it through one function per domain; for the NFT domain that function is `with_scarcity_transfers`, and for this layer it is C2's `with_coinage`, which adds both domains. The core then creates the durability layer from the list, and only after that starts this layer (G3), giving it the same `Running` handle it gave `with_coinage` (C2). The list cannot change while the durability layer runs. The core calls `with_coinage` exactly when a coinage configuration is set (A3 `ChainConfig`).

Three obligations follow, which the core and the host must meet and nothing checks:
- The two domain names never change between releases, because the durability layer stores the name with every record.
- The coinage configuration stays set at every start once the store holds a record of either domain. On a start without it, neither domain is listed, and the durability layer leaves their records untouched: nothing is lost, but no record is decided or built until a later start lists the domains again.
- The plug-ins can be called before this layer has started, so building waits behind a gate that G3 opens (C2 invariant 6, G3 invariant 5).

**What the durability layer offers.**
- **Registration.** One store transaction, which the durability layer opens and commits, registers one or more transactions of one domain under an operation. The layer always uses the form in which the records are planned inside that store transaction: a plan callback runs first and returns the policy parameters of each new record; the durability layer inserts one record per value, each *waiting to be built* with no bytes; then a registration callback receives the new records' identities, in the same order, and writes the coinage layer's rows (C1). Both callbacks run inside the store transaction, so they read exactly the state the registration commits against, and while it is open no other write runs (E3 invariant 1). They must be synchronous, must not wait for anything outside the store transaction, and must not call the durability layer. A refusal from either rolls everything back, the coinage layer's writes included. The layer never registers signed bytes: every attempt is built by the rebuild policy.
- **A record per transaction, never deleted.** Its status is one of *waiting to be built*, *submitted*, *succeeded on the best chain*, *succeeded finally*, or *failed* (expired, dispatch failed, rejected, or given up). Only *succeeded finally* and *failed* are stable. Statuses can be read and followed, for one record or for a whole operation.
- **Guarantees on those statuses.** A stable status never changes. *Succeeded finally* rests on a successful dispatch read in a finalized block, or on the completion oracle's proof at the finalized head. Each kind of failure rests on its own evidence: *expired*, on the window having closed with the attempt's absence proven, by the completion oracle at the finalized head or by reading every finalized block of the window; *dispatch failed*, on the attempt's failed dispatch read in a finalized block; *rejected*, on the chain's validation refusing the attempt before it was sent; *given up*, on the rebuild policy's decision. A failed transaction created nothing. Every transaction is mortal: valid from its birth block up to, but not including, its death block, which is the birth block plus its period. The period is a power of two from 4 to 65536, and for a period above 4096 the birth block's number modulo the period must be a multiple of the period divided by 4096. At most one attempt of a record is live at a time.
- **Retries through the domain's rebuild policy.** When an attempt is about to be recorded as expired, dispatch failed or rejected, the durability layer first asks the domain's rebuild policy whether to retry, giving it the record's policy parameters and the kind of failure. On yes, the same write records *waiting to be built* instead, so the failure is never visible, and the record keeps its identity. The durability layer asks the rebuild policy to build the records that are waiting, in rounds. For each record, the policy answers with the next attempt's signed bytes, gives the record up, which fails it, or does not answer, and the record keeps waiting. A record the policy does not answer is asked again 30 seconds later, or at once when a record of the same domain changes status. Before it rebuilds a record, the durability layer waits 5 seconds after the first attempt the policy built, doubling with each further attempt up to 600 seconds. It never stores signed bytes: an attempt that was live when the process stopped is decided from the chain, not sent again.
- **Background execution.** The durability layer keeps running while any record is open, even with the application closed, where the platform allows it (F2 invariant 2). It calls the coinage layer's plug-ins in the background too, so the plug-ins, and the keys they sign with, must be available in the background.

**What this layer supplies (C2), for each of its two domains.**
- **A completion oracle**: once per recovery pass, given the domain's submitted records and the pass's finalized and best heads, it answers for each record and each head two separate questions: does the chain prove that the transaction took effect, and does it prove that the transaction has not taken effect and cannot have done so and been undone since. *No* means only that there is no proof.
- **A rebuild policy**: it decides whether a failed attempt may be retried, and builds the bytes for the next attempt once the inputs are available. Every attempt consumes and creates exactly what was registered.

This boundary has two consequences for the rest of the document. **The layer never submits a transaction and never stores a status of its own.** Whether a coin is in use, spent or still arriving is derived from the durability layer's records and the coinage layer's rows (B1). **Registration is atomic**: a transaction never exists without the inputs it holds, and inputs are never held without their transaction.

## 2. How to read a component

Each component entry gives what the component **owns**, its interface, its **invariants**, and how it would be **verified**. The band says how much it costs to check the component:

| Band | Meaning |
|---|---|
| pure | Total function of its arguments. No state, no clock, no I/O. A `Snapshot` argument (B2) is one committed state that does not change, so reading it counts as reading an argument. |
| pure + clock | The same, with the current time as an argument. |
| stateful, effect-free | Owns state; performs no I/O. |
| pure decision with an effectful half | A pure decision plus a small part with effects; the entry states where the two divide. |
| effectful | Reaches the chain, the store, or the durability layer. |
| thin surface | Validates and delegates. Holds nothing durable; what it holds in memory, its entry lists under **Owns**. |

Nine components are pure or pure + clock: A1 to A5, B1, B3, D1 and E1. G1 is a thin surface over them. Three hold the coinage layer's durable state: B2, B4 and C1. The transactions' records and statuses belong to the durability layer. Four components are effectful: F1, F4, G2 and G3. Five more are split into a pure decision and a small effectful part: C2, E2, E3, F2 and F3. Three components hold state in memory only, which a restart discards: G1, G2 and G3.

**Notation.** Interfaces and data types are written in a notation that borrows Rust's syntax. The notation fixes meaning only: an implementation may be written in any language. Its constructs mean:
- `u8`, `u32`, `u64`, `u128`: unsigned integers of that many bits. `i8`: a signed 8-bit integer. `bool`: true or false. `[u8; N]`: exactly `N` bytes. `String`: UTF-8 text.
- `Vec<T>`: a sequence of values of type `T`. `(A, B)`: a pair. `()`: no value. `Option<T>`: a value of type `T`, or none. `Result<T, E>`: either a value of type `T` or an error of type `E`.
- `struct`: a record of named fields. `enum`: exactly one of the listed variants, each of which may carry fields. `X::V` names the variant `V` of `X`.
- `type X = T`: `X` is another name for `T`, used to say what a value means.
- `fn f(a: A) -> R`: an operation that takes an argument `a` of type `A` and returns a value of type `R`. `Stream<T>`: values of type `T` delivered over time, until the stream closes. Every open stream, whether a component serves it or follows it, is held in memory and ends at a restart. An entry's **Owns** lists only what a component holds beyond its open streams.
- `trait`: a set of operations that the caller supplies to the layer, for example a signer (G1 `Funding`).

Where a value of such a type is sent or stored as bytes, its encoding is SCALE [SCALE], which defines an encoding for each construct above. Names in square brackets, such as [SCALE], refer to section 8, which lists every external specification and implementation this document relies on.

## 3. Dependency order

A component may depend only on components above it.

```
A3 Chain parameters                              (root: depends on nothing)
 ├── A1 Denominations
 ├── A2 Namespace
 ├── A4 Privacy policy
 └── A5 Split planning  ◀── A1          ⟦ algorithm is a black box ⟧
          │
          ▼
D1 Unload-token supply  ◀── A3
          │
          ▼
B1 Asset classification  ◀── A4
          │
          ▼
B3 Selection  ──────── invokes A5 to decide reshaping ────────┐
          │                                                   │
          ▼                                                   │
B2 The ledger  ◀──────────────────────────────────────────────┘
          │
          ▼
B4 Operation model
          │
          ▼
E1 Planning    E2 Assembly  ◀── A2, A3
          │
          ▼
C1 Registration  ═══════▶  ⟦ durability layer: records and statuses ⟧
          │                              ║ asks
          ▼                              ▼
C2 Durability plug-ins: completion oracle, rebuild policy  ◀── E2 to build
          │
          ▼
E3 Operation start
          │
          ▼
F1 Observation    F2 Maintenance    F3 Offload phases    F4 Reconstruction
                  │
                  ▼
G1 Primitive API    G2 Subscriptions    G3 Initialization
```

Reading a fact that another component stored does not count as a dependency. Only calling a component does.

Two parts of the diagram have internals specified elsewhere, and they differ in kind. The durability layer is a *service the coinage layer registers with*: it holds the records, has effects of its own, and calls back into C2. This document specifies its boundary (§1) but not its internals. Split planning (A5) is a *pure function inside the coinage layer*; only its algorithm is specified elsewhere.

## 4. Component catalogue

### Band A — Vocabulary and value arithmetic
*Pure and stateless. A1, A2 and A3 define the vocabulary the rest of the document uses: values, keys and chain parameters. A4 and A5 make decisions that need no state.*

---

#### A1 · Denominations

**Owns** nothing. **Band:** pure.

A coin is worth `2^e` cents for an integer **exponent** `e`. A **cent** is the unit of the one coinage instance the layer serves (A3). The chain fixes the legal range of exponents, which `ChainParams` carries (A3), and the layer uses only its non-negative part: currently 0 to 14, so the smallest coin is 1 cent and the largest is 16,384 cents. An **amount** is a whole number of cents.

```rust
fn value(exponent: Exponent) -> Amount;                                     // 2^exponent cents
fn denominations(amount: Amount, params: ChainParams) -> Vec<Exponent>;      // the fewest coins, largest first
fn exponent(chain_value: i8, params: ChainParams) -> Option<Exponent>;       // None: negative, or outside the legal range
```

- **`value`** is `2^exponent` cents. Sums of values, such as a purse's total, are exact.
- **`denominations`** returns the coins that make up `amount` with the fewest coins, which is one multiset only. Let `m` be the largest legal exponent (A3). The result is `amount / 2^m` coins of exponent `m` (integer division), followed by one coin for each bit that is set in the remainder `amount mod 2^m`, largest first. An amount of zero gives no coins. B3 and E1 use it wherever an amount has to become coins, so the result must be the same in every implementation (P4).
- **`exponent`** turns the signed exponent the chain stores in a coin into an `Exponent`. It returns none for a negative exponent or one outside the legal range (invariant 1). F1 applies it to every coin value it reads. A coin whose value gives none is not a coin of this layer, so an import leaves it where it is (G1 invariant 5).

**Invariants**
1. The chain's exponent type is signed and allows coins smaller than a cent, but this layer does not use such coins (§6). The layer never creates a coin with a negative exponent, and `exponent` refuses one when a coin value is read. It does not round such a coin's value down, because that would silently misstate a balance.

**Verified by** round-tripping `denominations` against summation over the legal range, by fixed vectors for `denominations` at the boundaries of the range, and by checking that `exponent` refuses every negative and every out-of-range value.

---

#### A2 · Namespace

**Owns** nothing. **Band:** pure. A2 defines the derivation paths and computes keys from them. It stores nothing: B2 stores which installation is current, and earlier installations are published on chain (F4).

Every coin account and every recycler-entry key is derived from the user's recovery phrase. A key is identified by a tree and three values:

- **purse**: one isolated balance, a `u32`. The main purse has the reserved identifier `u32::MAX`; other purses count up from zero, so no assigned identifier can equal the main purse's. The same purse identifier can appear again in a later installation; a purse holds only the coins under the current installation (B2 invariant 5).
- **installation**: a random 32-byte value, created whenever the layer starts on a fresh local store (G3). The reason it exists is explained below, under *Why installations exist*.
- **item**: the index within one `(purse, installation)`, a `u32`.

There are two trees with separate roots, so that reconstruction can walk each tree without deriving keys that could only belong to the other. There is also one account outside both trees, for the installation registry (F4 invariant 4):

```text
coins      //coinage //<purse> //<installation> /<item>             sr25519; the item junction is soft
entries    //coinage-ring-vrf //<purse> //<installation> //<item>   bandersnatch; every junction is hard
registry   //datastore                                              sr25519
```

`//` marks a hard junction and `/` a soft one. A soft junction derives the child from the parent key pair and is much cheaper to compute than a hard one. Reconstruction derives thousands of coin indices per purse, so for coins, deriving the item keys is the whole cost of a scan. Entry derivation is a keyed hash at every step, so a hard junction costs no more there.

```rust
type Entropy       = Vec<u8>;    // E: the BIP-39 entropy, 16 or 32 bytes
type Sr25519Secret = [u8; 64];   // the secret scalar, then the nonce, 32 bytes each: the layout of G1's CoinSecret
type EntrySeed     = [u8; 32];   // eₙ below: the seed of an entry's secret
type EntryKey      = [u8; 32];   // an entry's public member key, as a ring lists it

fn coin_key(e: Entropy, purse: PurseId, installation: InstallationId, item: u32) -> (CoinSecret, AccountId);
fn entry_key(e: Entropy, purse: PurseId, installation: InstallationId, item: u32) -> (EntrySeed, EntryKey);
fn datastore_key(e: Entropy) -> (Sr25519Secret, AccountId);
```

- **`coin_key`** computes the key of the coin path `//coinage//<purse>//<installation>/<item>` by the rules below, and returns the coin secret and the coin's account, which is its public key. The scalar half of the secret and the account are the same on every call. The nonce half differs between calls, because the rules do not fix the nonce of a soft-derived key.
- **`entry_key`** computes the seed `eₙ` of the entry path `//coinage-ring-vrf//<purse>//<installation>//<item>` and the entry's public member key, the public key of the secret that `eₙ` gives. Both are the same on every call.
- **`datastore_key`** computes the key of `//datastore` and its account.

The operations take `E` itself. The host supplies `E` through the `Seed` interface given to `start` (G1 `Interfaces`), G3 holds that interface for the running layer, and A2 keeps nothing. E2 uses these operations to sign, G1's export to hand out coin secrets, and F1 and F4 to name the accounts and member keys they read. No operation returns a key above a coin or an entry in the tree.

**The derivation, exactly.** Both shipped apps compute these rules. A2 invariant 1 holds only if every implementation computes exactly these rules.

*The entropy.* Everything starts from `E`, the BIP-39 [BIP-39] entropy the recovery phrase encodes: 16 bytes for 12 words, 32 bytes for 24. The phrase's checksum is not part of `E`.

*Chain codes.* Every junction has a 32-byte chain code, defined by the junction's value as follows:

| Junction value | Chain code |
|---|---|
| A name: `coinage`, `coinage-ring-vrf`, `datastore` | The SCALE [SCALE] encoding of the name as a `String`, followed by zero bytes up to 32 |
| A purse or an item | The SCALE encoding of the number as a `u64`, followed by zero bytes up to 32 |
| An installation | Its 32 bytes, unchanged |

For example, `coinage` is `1c 63 6f 69 6e 61 67 65` followed by 24 zero bytes, and the main purse `u32::MAX` is `ff ff ff ff` followed by 28 zero bytes.

Note: Substrate's `sp-core` library [sp-core] deviates from this definition for the installation. Its path parser reads a `0x…` segment as text and SCALE-encodes the string, which gives a different chain code. An implementation that builds the path as a string and parses it with `sp-core` therefore derives the wrong keys.

*sr25519: coins and the registry account.* sr25519 is Schnorr signatures over the ristretto255 group [RFC 9496]. Let `ℓ` be the order of the group and `B` its base point. An sr25519 secret key is a pair: a secret scalar `k` with `0 ≤ k < ℓ`, and a 32-byte nonce, which only seeds the randomness of signatures. The public key is `P = k·B`, encoded in 32 bytes as RFC 9496 specifies. A scalar is encoded in 32 bytes, little-endian.

The rules below use three operations:
- `SHA-512` [FIPS 180-4] and `PBKDF2-HMAC-SHA512` [RFC 8018].
- A Merlin transcript [Merlin]. `T = Transcript(label)` starts a transcript, `T.append(label, bytes)` absorbs bytes under a label, and `T.challenge(label, n)` outputs `n` bytes that depend on everything absorbed before. Labels are ASCII strings, and the operations are exactly those the Merlin specification defines. The derivation uses Merlin only as a hash function with labelled inputs. It is fixed because sr25519 key derivation is defined this way [schnorrkel], and the keys that the shipped apps already hold depend on it.
- `expand(m)`, which turns a 32-byte mini secret `m` into a secret key. Let `h = SHA-512(m)` and `a = h[0..32]`. Clamp `a` as Ed25519 clamps its secret scalar [RFC 8032, section 5.1.5]: clear the lowest three bits of `a[0]`, clear the highest bit of `a[31]` and set its second-highest bit. Read `a` as a little-endian integer; the secret scalar is `k = a / 8`, a division without remainder whose result is below `ℓ`. The nonce is `h[32..64]`.

The keys are then:
1. **The root.** The root mini secret `m₀` is the first 32 bytes of `PBKDF2-HMAC-SHA512(password = E, salt = "mnemonic", 2048 iterations)`. The root key is `expand(m₀)`.
2. **A hard junction** with chain code `c` computes the child's mini secret from the parent's secret scalar `k`: `T = Transcript("SchnorrRistrettoHDKD")`, then `T.append("sign-bytes", "")`, `T.append("chain-code", c)`, `T.append("secret-key", k)`, and `m = T.challenge("HDKD-hard", 32)`. The child key is `expand(m)`. The parent's nonce is not used.
3. **A soft junction** with chain code `c` adds a tweak to the parent's key: `T = Transcript("SchnorrRistrettoHDKD")`, then `T.append("sign-bytes", "")`, `T.append("chain-code", c)`, `T.append("public-key", P)`, and `s = T.challenge("HDKD-scalar", 64)`, read as a little-endian integer modulo `ℓ`. The child's secret scalar is `k + s mod ℓ`, and its public key is `P + s·B`. Anyone who knows `P` and `c` can compute the child's public key, without any secret. A soft-derived key has no mini secret.

The secret scalar and the public key of every key are fixed by these rules. The nonce of a soft-derived key is not: the reference implementation computes it from the parent's nonce, the parent's secret and fresh randomness, so it differs each time a coin key is derived. Every coin key is soft-derived, so the 32 nonce bytes of a coin secret differ between derivations, while its 32 scalar bytes do not.

The reference implementation of these rules is the `schnorrkel` library [schnorrkel]: step 2 is its `hard_derive_mini_secret_key` and step 3 its `derived_key_simple`, each with an empty extra input, and `expand` is its `Ed25519` expansion mode.

*Bandersnatch: entries.* An entry key is a key on the Bandersnatch curve, used for the ring proofs of the recycler [verifiable]. Bandersnatch keys have no derivation scheme of their own, so the entry's secret is computed from the entropy by hashing once per junction:
1. `e₀ = E`, the entropy itself, not `m₀`.
2. For each junction in order, `eᵢ₊₁ = BLAKE2b-256(key = chain code, message = eᵢ)`: keyed BLAKE2b with a 32-byte output [RFC 7693].
3. The entry's secret scalar is computed from the 32-byte seed `eₙ` by the function `Secret::from_seed` of the `ark-vrf` library, version 0.5.1, for the suite `Bandersnatch-SHA512-ELL2-v1` [ark-vrf]. No specification defines this function apart from that implementation, so the design pins its version. The Android app and this repository's core both reach it through the `verifiable` library's `new_secret`.


*Test vectors*, from the Android app's tests, for the phrase `bottom drive obey lake curtain smoke basket hold race lonely fit walk`:
- `//datastore` has the public key `0x82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d`.
- The coin of the main purse, installation `0x5a` repeated 32 times, item 7, has the public key `0x4eced11d18ac64e54559cc45eb79415ca66d9354e74394dc8729b69a0fbd9107`.

**Invariants**
1. The same recovery phrase, tree, purse, installation and item always give the same secret scalar and the same public key, by the rules above, in every implementation. The nonce of a coin secret is not covered, because the rules do not fix it.

**What it relies on.** The derivation paths are safe only if three conditions hold. A2 cannot check them itself; other components keep them:
- **No derived key is used twice** (P5). An item is never allocated twice within one `(purse, installation)`, not even after its purse is deleted (B2 invariant 4), and an installation identifier is never drawn twice (G3 invariant 3). This holds even after the coin or entry under a key has been spent. A spent coin's account may already have been shown to a payee, and a spent entry's key stays in a public ring list, so using the key again would link new activity to old activity. Purse identifiers do not need this rule. A caller keeps the purse identifier it was given and presents it again after a reinstall. This reuses no key, because the installation comes below the purse in every path: purse 7 under one installation and purse 7 under another installation lead to different keys.
- **Allocation happens only in the current installation** (B2 invariant 4). The layer reads earlier installations, and finds and spends their coins, but never allocates new items in them. What a reconstruction finds in an earlier installation never advances a counter (F4).
- **The installation node's public key never leaves the layer** (P6): no primitive returns it (G1 invariant 5), nothing logs it (G3 invariant 6), and the registry publishes only the installation identifier (F4 invariant 4). Keeping this key private is what makes the soft junction for coin items safe, because recovering a parent secret from a soft child's secret requires the parent's public key.

**Why installations exist.** The rule that no derived key is used twice must hold even after the local store is lost. A counter alone cannot guarantee that, because the counter is kept in the local store and is lost with it. After a reinstall on the same seed, allocation would restart at zero and re-derive items whose coin secrets were already handed to payees, which would give those payees keys to coins they do not own. A fresh random installation identifier puts the new instance's allocations in a namespace where nothing was allocated before, whatever its counter says. So the rule needs no record of how far the previous instance allocated; it needs only that the two installations' namespaces differ.

**Verified by** fixed derivation vectors for both trees, and a property test that no two distinct `(purse, installation, item)` triples collide.

**Open question.** The safety of the soft junction for coin items rests on an argument, not on a proof, and this document records no review of the argument. By step 3, a coin's secret scalar is `k_coin = k_inst + s`, where `k_inst` is the installation node's secret scalar and the tweak `s` is computed from the coin's chain code and the installation node's public key `P_inst`. A payee who receives a coin secret learns `k_coin`. If the payee also knew `P_inst`, the payee could compute `s`, then `k_inst = k_coin − s`, and from `k_inst` the secret of every coin under that installation. The design's argument is that nobody outside the layer can learn `P_inst`. The layer never publishes it (the third condition above). It cannot be computed from the root public key, because the installation node lies below three hard junctions. And it cannot be computed from coin public keys or coin secrets, because each coin public key is `P_inst + s·B`, where `s` is itself a hash of `P_inst`. A cryptographer should review whether the argument holds against an attacker who holds the secrets of many coins under one installation, their public keys and their item numbers. If the argument fails, one coin secret exposes every coin under the same installation.

---

#### A3 · Chain parameters

**Owns** nothing. **Band:** pure. A3 is two data types and one function. `validate` checks the runtime the layer connects to against the layer's configuration and returns `ChainParams`, or rejects the runtime. A3 holds no value of either type; G3 holds the `ChainParams` from the last check that passed. No component takes a chain limit, a call index or an origin index from anywhere but `ChainParams`, so a runtime change can break only A3.

- **`ChainConfig`**: the per-network configuration, fixed when the layer is constructed. It gives the identifier of the coinage instance the layer serves. The runtime cannot say which of its instances that is, so the configuration must. It also gives every limit the runtime does not publish, and gives the rescue margin (F2 invariant 1), the share of the allowance kept as the token reserve (D1 invariant 2), the fallback age cutoff (below) and the longest validity window the layer gives a transaction.
- **`ChainParams`**: the validated parameters. Only `validate` produces a value of this type, so holding one means every check below passed. A value contains the configuration; the limits the runtime publishes; the instance's external asset and its `asset_unit`, the amount of the external asset in one cent; and the index of every call and origin the layer uses (§1, the chain).
- **`validate(runtime_description, instance_record, maximum_age, config)`** → `ChainParams`, or `UnsupportedRuntime` naming the check that failed. G3 reads the runtime description, the record of the configured instance and the runtime's maximum coin age from the chain and calls it at connection, and again whenever F1 observes a new runtime version. If the chain has no instance with the configured identifier, there is no record, and `validate` fails.

The **limits** in `ChainParams` are: the legal exponent range, coins per split, entries per unload, entries per unpaid load (the batch a top-up's load may carry), entries per reload (the fresh entries one unload may load with its surplus, which the chain bounds by the per-split output cap), ring sizes and ring lifetime, the age cutoff, the unload allowance and its period (D1), the external asset's minimum balance, and the lock periods applied after a failure.

The **age cutoff** is the coin age from which the layer no longer spends a coin and only recycles it (B1, F2). It is two below the maximum coin age the runtime reports. `maximum_age` is none when the runtime reports no maximum or the read fails, and the cutoff is then the fallback `ChainConfig` gives, 14 in the shipped configuration. Both rules are the shipped wallet's (§5).

**Invariants**
1. Calls and origins are looked up **by name** in the runtime description, never by position. If a name is missing, `validate` fails. A reordered enumeration in the runtime therefore never makes the layer select a different call or origin.
2. If a limit the runtime publishes disagrees with the configured value, `validate` fails. A limit the runtime does not publish has no default: `ChainConfig` must give it, and `validate` fails if it is missing. Among these are the values that guard the one way value can be lost (an entry's ring expiring). The age cutoff is the one limit whose read may fail without failing `validate`: it then takes the configured fallback (P12 states what that risks). The longest validity window must not exceed the protocol's maximum claim validity window (RFC deviations, gap 5), because a payer's reclaim waits only that long for a claim made in time. It must also be a power of two from 4 to 65536, because the durability layer accepts only such periods (§1).
3. `validate` also checks four relations between values: that the exponent range is well formed; that **the per-split output cap exceeds the exponent range**, which A5 depends on; that the per-reload cap is at least the number of legal exponents, which F3 invariant 3 depends on (the per-unpaid-load cap may be smaller, because E1 cuts a top-up's load into batches of that size); and that the rescue margin exceeds one token period, the longest validity window and the engine's longest wait before rebuilding a record (§1), which P1 and P10 depend on.

**Verified by** checking each assumption against a live runtime's description, and by fixed vectors for every proof message and every identifier the layer derives rather than reads.

---
#### A4 · Privacy policy

**Owns** nothing; B2 stores the policy. **Band:** pure.

Coinage makes the user choose between privacy and availability: value held back longer is harder to link to its origin, and value that can be spent sooner is easier to use. The user makes this choice in their own settings; a product never makes it. This component is the form in which the choice reaches the layer: readiness rules, a recycle age, a cap, two permissions and a switch, carried as data. The user picks one of three presets (below), and the components read only the values, never a preset's name.

```rust
struct PrivacyPolicy {
    ready_when: Vec<ReadyRule>,             // an entry is ready when any rule holds
    recycle_at: RecycleAge,
    max_unavailable_share: u8,              // percent of a purse's total, 0 to 100
    held_back_for_user: HeldBackSpend,      // a spend the user starts (Initiator::User)
    held_back_for_products: HeldBackSpend,  // a spend layer 2 makes for a product (Initiator::Product)
    move_through_recycler: bool,            // how rebalance and delete_purse move value (G1)
}
struct ReadyRule { members: MemberThreshold, dwell: Duration }
enum MemberThreshold { Count(u32), PercentOfRingSize(u8) }
enum RecycleAge { Age(u32), CutoffDividedBy(u32) }
enum HeldBackSpend { Never, AfterConfirmation, Always }
enum Initiator { User, Product }            // who asked for a spend (G1 export and offload)
enum PolicyError { NoReadyRule, PercentAbove100, ZeroDivisor }

fn check(policy: PrivacyPolicy) -> Result<(), PolicyError>;
fn effective(policy: PrivacyPolicy, params: ChainParams) -> PrivacyPolicy;
```

- **`ready_when`**: when an entry counts as ready (B1). A rule holds when the ring's root includes at least `members` members, and `dwell` has passed since the entry's own key was first observed included in that root. `Count(n)` is `n` members. `PercentOfRingSize(p)` is `p` percent of the ring size A3 gives for the entry's denomination, rounded up. An entry is ready when any rule holds.
- **`recycle_at`**: the coin age from which the recycling sweep takes a coin, as far as the cap allows (B1 `recycle_due`, F2). `Age(a)` is the age `a`. `CutoffDividedBy(d)` is the age cutoff (A3) divided by `d`, rounded down, and at least 1, so it follows the chain's maximum.
- **`max_unavailable_share`**: how much of a purse may be unavailable when the recycling sweep admits another coin (B1 `recycle_due`).
- **`held_back_for_user`** and **`held_back_for_products`**: whether a spend may take held-back value (B1 `HeldBack`) when the value that is not held back does not cover it. The first applies to a spend the user starts, the second to a spend layer 2 makes for a product; the caller names which (G1 `export` and `offload`). `Never`: the spend does not take held-back value. `AfterConfirmation`: it takes held-back value only once the user has confirmed it; without the confirmation, `export` and `offload` fail with `NeedsConfirmation` and start nothing (E3, F3). `Always`: it takes held-back value without asking.
- **`move_through_recycler`**: whether `rebalance` and `delete_purse` move value through the recycler. Keys are derived under the purse, so these primitives move value with a transfer on chain. A direct transfer is immediate, but it links the two purses' coins for anyone who knows the accounts on one side, and layer 2's clearing references tell a receiving product those accounts. Through the recycler, the two purses' coins are not linked, at the cost of a privacy delay and recycler capacity.

- **`check`** refuses a policy with no readiness rule, with a percentage above 100 (the share or a ring percentage), or with a divisor of zero. G1's `set_policy` refuses such a policy, and B2 stores any other policy exactly as given.
- **`effective`** applies the first outer limit below to a stored policy: it resolves `recycle_at` to `Age(a)`, with `a` at most the age cutoff in `params` (A3). The second outer limit, the token reserve, is applied by D1 and is not a field of the policy. The *effective policy* is `effective` applied to the policy B2 stores, with the `ChainParams` G3 holds. Every component that reads the policy reads the effective policy, so a runtime upgrade that lowers the cutoff takes effect without a stored change.

**The three presets** are the shipped wallet's settings, so the same choice does the same thing on every host (§5). A host that follows the shipped wallet offers these three and passes *Fastest* as the initial policy (G1 `Interfaces`). No code checks which preset the stored policy matches.

| Field | Fastest | Balanced | Most private |
|---|---|---|---|
| `ready_when` | 0% of the ring size, no dwell | 20% of the ring size, no dwell; or 32 members and 10 minutes | 90% of the ring size, no dwell; or 32 members and 10 minutes |
| `recycle_at` | `CutoffDividedBy(1)`: at the age cutoff | `CutoffDividedBy(3)` | `Age(1)` |
| `max_unavailable_share` | 0 | 20 | 100 |
| `held_back_for_user` | `AfterConfirmation` | `AfterConfirmation` | `Never` |
| `held_back_for_products` | `Always` | `AfterConfirmation` | `AfterConfirmation` |
| `move_through_recycler` | false | true | true |

The shipped wallet has one purse and no moves between purses, so it sets no value for `move_through_recycler`; the values above are this design's.

**Two limits are outside the policy**, and a caller cannot widen them:

1. **The age cutoff.** From the cutoff on, the layer does not spend a coin; it only recycles it (B1, F2). So no policy may postpone recycling past the cutoff, and `effective` enforces this.
2. **The token reserve.** While the period's free unload tokens are no more than the reserve, the recycling sweep admits no coin, whatever the policy says (D1 invariant 2). The policy has no field for it.

**Invariants**
1. The policy is **data, not behaviour**. Every component that reads it (readiness and the recycling admission in B1, the recycling sweep in F2, held-back spending in E3 and F3, moves between purses in E1) has its own logic and differs only in the numbers it is given.
2. Limits the user cannot influence are **not** exposed as policy. A setting that cannot be changed would mislead the user. The order in which selection tries its strategies is one of them: it is fixed by who asked for the spend (B3 invariant 2).

**Verified by** checking each preset against the shipped wallet's behaviour on the same holdings, that each preset is expressible as parameters with no special cases downstream, that `effective` clamps a policy that exceeds the age cutoff, and that a policy change alters classification without changing a record.

---

#### A5 · Split planning

**Owns** nothing. **Band:** pure.

Coinage can split coins but never merge them, so an amount that no subset of held coins sums to can only be reached by splitting coins first. This component decides which coins to split and how. How the plan is found is not specified: any algorithm that returns the plan defined below conforms, and the layer depends only on the contract. One such algorithm is described in two places: an [intuitive explanation](https://github.com/paritytech/host-coinage-design/blob/main/intuitive-split-algo.md), and a [rigorous treatment with correctness proofs](https://github.com/paritytech/host-coinage-design/blob/main/formal-split-algo.md).

```rust
struct Split { consume: Exponent, produce: Vec<Exponent> }   // split one coin of `consume` into coins of these exponents

fn plan_splits(amount: Amount, counts: Vec<(Exponent, u32)>, params: ChainParams) -> Option<Vec<Split>>;   // None: not coverable
```

- **`plan_splits`** takes the amount, the number of splittable coins of each exponent, and the chain parameters, which give the legal exponent range and the per-split output cap (A3). It returns the one plan the contract below defines, or none when no plan exists.

The planner receives counts, never records: a split names the exponent of the coin it consumes, not a coin. B3 chooses which coin of that exponent is consumed, so record identity is decided in one place only.

**The contract**
1. **Conservation.** Each split's outputs sum in value to its input.
2. **Sufficiency.** After the plan is applied, some subset of the resulting coins sums exactly to `amount`. This is the only property the layer needs for correctness.
3. **Independence.** The splits consume distinct coins, and **no split's input is another split's output**, so they can all be submitted at once. This is why a payment that reshapes coins creates no dependencies between its transactions and needs no ordering.
4. **Boundedness.** Outputs per split never exceed the chain's cap, and the number of splits is bounded by the exponent range alone.
5. **Determinism.** The plan is fully defined, so every implementation returns the same plan for the same amount, counts and parameters. Let `m` be the largest legal exponent (A1), and for each exponent `j` from 1 to `m` let `below(j)` be the total value of the counted coins whose exponent is less than `j`. Exponent `j` is **forced** when `amount mod 2^j` exceeds `below(j)`. Every coin of exponent `j` or more is a multiple of `2^j`, so the coins below `j` must pay `amount mod 2^j` on their own; when they are worth less, some coin of exponent `j` or more has to be split. A **run** is a maximal set of consecutive forced exponents. The plan has one split for each run and no other splits, listed in ascending order of the exponent they consume. The split for the run from exponent `a` to exponent `b` consumes one coin of exponent `b` and produces two coins of exponent `a - 1` and one coin of each exponent from `a` to `b - 1`, listed in ascending order. When no exponent is forced, the plan is empty. The [rigorous treatment](https://github.com/paritytech/host-coinage-design/blob/main/formal-split-algo.md) proves that the counts include a coin of exponent `b` for every run, and that no plan creates fewer new coins (Theorems A and B), so contract 6 holds. The case of equality in the proof of Theorem A also shows that every plan creating that few coins has at least one split per run, and that the only one with exactly one split per run is this plan.
6. **Minimality.** No other plan creates fewer new coins. This is a *quality* guarantee, kept separate from correctness on purpose: a sufficient but wasteful plan is still correct, and this separation is what makes the planner replaceable.

**What it relies on.** First, that `amount` does not exceed the total value of `counts`; B3 ensures this. Under that precondition *not coverable* is never returned: taking whole coins, largest first, until the next coin would overshoot, and splitting that coin into `2^(k-1), …, 2^0, 2^0`, reaches any amount with one level of splits within the cap. B3 invariant 4 relies on this. Second, that the chain's per-split output cap exceeds the exponent range (A3 invariant 3). Without it, a plan could need a split of another split's output, and independence would no longer hold. Third, that the caller applies the plan exactly as returned: not merged to save a transaction, not reordered, and not replaced by a similar shape, because every property above holds only for the exact plan (B3 invariant 3).

**A deliberate trade-off.** Minimising coins is not the same as minimising transactions: a heuristic that splits a single coin always costs one transaction, while an optimal plan may use several. The layer takes the optimal plan for two reasons. The splits are independent, so they are submitted in one round rather than one after another. And every extra coin has a lasting cost: it occupies an account, ages toward the age cutoff, and may need reshaping later. A transaction is paid for only once.

**Verified by** checking conservation and sufficiency on generated holdings, that no split consumes another's output, the coin count against an independent optimum on exhaustive small cases, and fixed vectors for which of several minimal plans is returned.

---

### Band B — The ledger
*B2 and B4 hold the coinage layer's own durable state; B1 and B3 are pure functions over it. C1 holds the rest of the durable state (band C). An **operation** is one request, by a caller or by the layer itself, such as a payment, a top-up or a sweep. One operation may need several chain transactions to finish.*

---

#### B1 · Asset classification

**Owns** nothing; it classifies a single coin or entry. **Band:** pure + clock.

A **coin** is a chain account holding one denomination. A **recycler entry** is a key placed in a ring. An entry can later be unloaded back into coins or into an external asset.

```rust
enum CoinClass  { Exported, Void, Spent, InUse(OpKey), Minting, Available }
enum RingAxis   { Missing, Waiting, Ready, Degraded(u32), Expired }   // Degraded: the ring closed with this many members
enum EntryLocal { Minting, Available, InUse(OpKey), Consumed, Void }
enum Class { Coin(CoinClass), Entry { ring: RingAxis, local: EntryLocal } }

enum Observed { Never, At(BlockRef), FailedSince(BlockRef) }        // the third axis, below

enum HeldBack { Dwell, RingBelowThreshold, RecycleDue }      // unavailable by the policy's choice
enum Arriving { NeverObserved, Minting, Locked, NotIncluded, RescueMargin, AgeCutoff }   // unavailable, by no one's choice
enum Selectability {
    Now,                        // selectable at `now`
    HeldBack(HeldBack),
    Arriving(Arriving),
    Never,                      // in no bucket: in use, spent, void, exported, expired, consumed
}

struct AssetState {
    class: Class,
    observed: Observed,
    selectability: Selectability,
    export: Option<ExportStatus>,   // G1's type; for an exported coin only
}

fn classify(asset: Asset, view: Snapshot, now: Time, params: ChainParams) -> AssetState;
fn recycle_due(view: Snapshot, purse: PurseId, now: Time, params: ChainParams) -> Vec<Asset>;
```

- **`classify`** computes one asset's state from the facts in the table below, all read from the snapshot `view`, from `now`, and from `params`, which give the age cutoff, the ring sizes and the rescue margin (A3). It reads the stored policy from the same snapshot and applies A4's `effective` to it with `params`. For a coin it also calls `recycle_due` for the coin's purse. It stores nothing. B2's `balance` puts the asset into a bucket by its `selectability`; B3 selects by it; C2's `prepare` builds only when every input is `Now`; G2 reports `export`. A coin a peer sent is not one of this layer's assets, and `classify` is never asked about one.
- **`recycle_due`** returns the coins of `purse` that the recycling sweep takes now (F2), and in that order. It returns none while D1's `running_low` holds for the current supply (D1 invariant 2). Otherwise it walks the purse's coins of class `Available` whose observed age is at least the effective `recycle_at` (A4), oldest first, ties broken by installation and index. It admits a coin while the *unavailable value* is below `max_unavailable_share` percent of the *purse total*, and each admitted coin adds its value to the unavailable value, so the last coin admitted may take the unavailable value past the cap. The purse total is the value of every coin and entry of the purse that the `Never` row of the table below does not cover. The unavailable value starts as the value of the purse's coins of class `Minting`, plus its entries that are neither selectable now nor covered by the `Never` row. None of these values depends on a coin's selectability, so `recycle_due` does not depend on itself. A coin at the age cutoff is walked like any other; F2 recycles it whether or not it is admitted.

**No asset has a stored status.** An asset's state is computed from facts that each have one owner, all read from one snapshot:

| Fact | Owner |
|---|---|
| The transaction that creates it, and that transaction's status | The durability layer's record, with C1's output row (C1 `creator`) |
| A transaction that consumes it and has not failed, and its status | The same, with C1's input row (C1 `consumers`) |
| Whether its secret was handed out | B2's disclosure mark |
| What the chain last showed: present or not, its age, a lock, its ring, and for an exported coin the transaction that emptied its account | B2's observation, fed by F1 |
| For an entry: the instant its key was first observed included in its ring's root | B2 |

A stored status would be a second copy of these facts. Every event that changes one of the facts (a verdict, a reorganisation that withdraws a success, a rebuild, a failure that frees inputs, a restart) would have to update the copy in the same commit, on every code path. If one path missed it, a spent coin would stay spendable or a coin would stay locked for good.

A coin's class is the first line that applies:

```
Exported    its secret was handed out — see its export status below
Void        its creating transaction failed; the account was never funded
Spent       its consumer succeeded finally
InUse(op)   an input of a transaction that has not failed
Minting     created by a transaction that has not failed and has not succeeded finally
Available   present on chain; nothing consumes it
```

The order decides the overlaps. For example, a coin still minting may already be an input of another operation (B3), and then it classifies as `InUse`. An export hands out a coin that is still minting only through a transaction of its own, never as it is.

A coin is selectable (`Now`) when it is `Available`, any chain lock on it has elapsed, its age is below the age cutoff (A3), and `recycle_due` does not return it. The age matters because every use except recycling transfers or splits the coin. `Exported` is checked first, so a coin whose secret was handed out is never `Available` again, whatever happens to its transactions, and nothing selects it. An exported coin must be below the age cutoff when it is minted, including a coin that a split produces for the export, because the payee's claim is one more transfer.

**A coin a peer sent is not one of this layer's assets.** It has no index here (C1 invariant 6), it is only an input of its import, and it counts in no bucket. The import's outputs count like any other output. They count at the exponent observed on chain once they land, never at the value the sender declared.

**An exported coin is still observed.** Once its secret has been handed out, it is never selectable again (C1 invariant 4). But the layer keeps observing its account until the coin has either been claimed or come back. The sender needs to know whether the payee claimed it, and only an unclaimed coin is worth reclaiming. Its export status is one of:

| Status | Meaning |
|---|---|
| Not yet minted | The transaction creating it has not finally succeeded |
| Awaiting the payee | Present on chain |
| Claimed | Moved by the payee, by the named transaction: either the transaction that emptied the account, as observed, or this layer's own import of the coin. Final once that move is stable |
| Reclaimed | Moved back by this layer's reclaim. Final once the reclaim is stable |
| Never minted | The transaction creating it failed, or a reclaim cancelled it. The secret controls nothing |

An entry has **two independent axes**. Reading one axis as the other loses information that an operation needs. The ring axis is observed. The local axis is computed in the same way as a coin's class:

```
in the ring:   Missing ──▶ Waiting ──▶ Ready
                               └─────▶ Degraded(n)   ring closed to new members with n, below every rule's member count
               any ──▶ Expired                       ring cleaned up while the entry was unconsumed

locally:       Minting │ Available │ InUse(op) │ Consumed │ Void
```

`Ready` needs one of the policy's readiness rules to hold (A4 `ready_when`): the ring's root includes at least the rule's members, **and** the rule's dwell has passed since this entry's key was first observed included in that root. So that instant is stored per entry (B2). With the member count alone, an entry could be spent the moment its ring fills, and an observer could then link the entry to the moment the ring filled. Preventing that link is the purpose of the dwell. The layer adds no random delay of its own: the shipped wallet has none, and the dwell is the one timing protection the policy sets.

An `Expired` entry's value is gone. It counts in no bucket and nothing selects it.

An entry is selectable (`Now`) when all of these hold: it is locally `Available`, it is `Ready` in the ring, its key is included in the ring root (invariant 4), any chain lock on it has elapsed, and its ring is not within the rescue margin of expiry (F2 invariant 1). An entry whose ring is within the rescue margin is left to the rescue sweep. A `Degraded` entry is never `Ready`, because no rule can hold for it; it is held back, so only a spend that may take held-back value takes it (A4).

When an asset is not selectable now, `classify` names the first condition that fails, and that condition decides between *held back* and *arriving*:

| Condition that fails | Selectability |
|---|---|
| Never observed | `Arriving(NeverObserved)` |
| Coin `Minting`, entry locally `Minting`, or ring axis `Missing` | `Arriving(Minting)` |
| A chain lock has not elapsed | `Arriving(Locked)` |
| Entry in a ring, its key not yet included in the ring root | `Arriving(NotIncluded)` |
| Ring within the rescue margin | `Arriving(RescueMargin)` |
| Coin at or past the age cutoff | `Arriving(AgeCutoff)` |
| Coin returned by `recycle_due` | `HeldBack(RecycleDue)` |
| Some rule's member count reached, but no rule's dwell passed | `HeldBack(Dwell)` |
| No rule's member count reached: ring `Waiting` or `Degraded` | `HeldBack(RingBelowThreshold)` |
| Class `InUse`, `Spent`, `Void`, `Exported`, `Consumed`, or ring `Expired` | `Never` |

The `Arriving` rows are checked before the `HeldBack` rows, so a holding that fits both is *arriving* (B2). Whether an entry has a token this period is not B1's to decide; B2's `balance` applies it.

Only two parties can make a `Degraded` entry spendable, and a product is neither of them. The **rescue sweep** (F2) may unload a `Degraded` entry, because otherwise the chain destroys it. And the **user** may let a spend take held-back value, which trades anonymity for availability. That choice is a privacy preference like the others, so it arrives with the policy (A4 `held_back_for_user` and `held_back_for_products`), and `AfterConfirmation` asks the user each time. No primitive accepts a product's decision on it.

**A third axis applies to coins and entries alike: has the chain reported on the asset at all?**

```
Never observed ──▶ Observed at block H ⇄ Read failed since H
```

What the chain reported, and whether it reported at all, are stored separately. If the two were merged, a lagging subscription could make an owned coin disappear from a balance: with no state for *not yet known*, the record would fall back to the default value.

**Invariants**
1. A coin or entry that was **never observed** has a state of its own, and is never read as zero. Its denomination is known locally, because the layer derived it; only the chain's confirmation is missing. An import's output is the exception: its value is the one observed on chain.
2. **An asset is in use exactly while it is an input of a transaction that has not failed**, and it is spent only once that transaction has succeeded finally. Nothing else holds an asset. The hold is the registration itself, so the hold is durable and survives a restart, and it ends by itself when the transaction fails for good. A failed attempt does not end the hold. While the rebuild policy allows another attempt, the durability layer returns the record to *waiting to be built* under the same identity, so its inputs stay `InUse`. Freeing them would let another transaction use them, and the runtime would refuse that transaction after an unload token was already spent building it. A success on the best chain does not settle anything either, because a reorganisation can withdraw the success and make the input unspent again.
3. Whether an asset is selectable is computed from the current time each time it is read, never stored as a flag. Three of an entry's five conditions change with the clock alone: the chain lock, the dwell inside readiness, and the rescue margin. So an entry can become selectable, or stop being selectable, without any chain event and without any record changing. Readiness is defined here, from stored observation, A4's thresholds and the current time, and it is computed when read. F1 stores only the facts that readiness is computed from. If readiness were computed once at observation and stored, an entry whose only remaining condition is the dwell would never become ready.
4. **An entry is selectable only once its key is included in its ring's root.** A ring is paged, and a member's position is reported within its page (§1). So the ring's included count is compared against `page × page_size + position`, not against the position alone. Comparing against the position alone reads a key that is not yet included as included; a proof built for it is then refused, and the rejection gives no reason.

**What it relies on.** The facts it classifies are read together from one committed state (B2 invariant 3), never from separate reads that could fall on both sides of a commit.

**Verified by** exhaustive tables: every combination of facts against its classification and selectability, including combinations that must not occur.

---
#### B2 · The ledger

**Owns** what only this layer knows about its purses and assets: every purse's metadata and state, every coin and entry's identity (purse, installation, index, kind, denomination), what the chain last showed for each asset and for each unload-token identity, the instant each entry's key was first observed included in its ring's root, the secrets of the peer coins an import is claiming until each claim is final (G1 invariant 5), the derivation counters, which installation is current, which earlier installations are known and how far each has been scanned, the disclosure marks, the privacy policy (A4), and the counter for the ids of the layer's own operations (F2 invariant 3). **Band:** stateful, effect-free.

None of this state survives a wipe, except the purse metadata, which may be backed up (§1). The rest is not meant to survive. A reconstruction finds earlier namespaces through the **installation registry**, which is published on chain rather than held here (F4).

**What it does not own** is any asset's status. Whether an asset is in use or spent comes from the durability layer's records and C1's rows, and B1 computes it. **Three components divide what an operation touches, and no fact belongs to two of them:** B4 owns the operation's identity, C1 owns the rows that tie its transactions to the assets they consume and create, and B2 owns the purses' and assets' own facts.

```rust
trait Snapshot {}     // one committed state of the shared store, read-only; every read of B2, C1, B4 and
                      // the durability layer's record statuses can be asked of it, all from that one state

enum AssetKind { Coin, Entry }
struct AssetRecord { asset: Asset, exponent: Exponent }

enum PurseState {
    Open,
    Deleting { drain_into: PurseId },    // G1 invariant 4: no new destination, no second deletion
    Closed { drained_into: PurseId },
}
struct PurseRecord { id: PurseId, name: String, created: Time, tag: Vec<u8>, state: PurseState }

// what F1 reports; all reads of one call are at one block (F1 invariant 5)
struct Facts { at: BlockRef, reads: Vec<(Subject, Read)> }
enum Subject { Asset(Asset), Token(TokenId) }
enum Read {
    Failed,                                         // the read did not return (invariant 6)
    Coin { now: Option<CoinOnChain>, emptied_by: Option<TxHash> },   // now None: the account is empty;
                                                    // emptied_by: for an exported coin, the transaction that emptied it
    Entry(Option<EntryOnChain>),                    // None: the key is in no ring
    Token { consumed: bool },
}
struct CoinOnChain { exponent: Exponent, age: u32, locked_until: Option<Time> }
struct EntryOnChain {
    ring: u32, revision: u32, page: u32, position: u32,     // §1: a position is reported within its page
    members: u32, included: u32, closed: bool, expires_at: Time,
    locked_until: Option<Time>,
}

enum Mark { Uncommitted, Committed }
struct ScanProgress { first_scan_done: bool, resume: Vec<u8> }   // `resume` is F4's, opaque here

// purses (all writes run inside a store transaction of the caller)
fn create_purse(txn: StoreTxn, name: String, tag: Vec<u8>, now: Time) -> PurseId;
fn purse(id: PurseId) -> Option<PurseRecord>;
fn purses() -> Vec<PurseRecord>;
fn begin_delete(txn: StoreTxn, purse: PurseId, drain_into: PurseId) -> Result<(), Error>;
fn close_purse(txn: StoreTxn, purse: PurseId);
fn resolve_purse(purse: PurseId) -> PurseId;

// assets
fn allocate(txn: StoreTxn, purse: PurseId, kind: AssetKind, outputs: Vec<Exponent>) -> Vec<Asset>;
fn adopt(txn: StoreTxn, found: Vec<AssetRecord>);                 // F4
fn asset(asset: Asset) -> Option<AssetRecord>;
fn assets(purse: PurseId) -> Vec<AssetRecord>;                    // invariant 5: the main purse includes earlier installations
fn observe(txn: StoreTxn, facts: Facts, now: Time);

// secrets of peer coins being imported
fn keep_secrets(txn: StoreTxn, secrets: Vec<CoinSecret>);
fn secret(coin: AccountId) -> Option<CoinSecret>;
fn forget_secrets(txn: StoreTxn, coins: Vec<AccountId>);

// disclosure marks
fn mark_disclosed(txn: StoreTxn, assets: Vec<Asset>) -> Result<(), Asset>;   // Err: the asset is already marked or held
fn commit_disclosed(txn: StoreTxn, assets: Vec<Asset>);
fn release_disclosed(txn: StoreTxn, assets: Vec<Asset>);          // uncommitted marks only
fn uncommitted() -> Vec<Asset>;
fn mark(asset: Asset) -> Option<Mark>;

// installations, policy, ids, backup
fn current_installation() -> Option<InstallationId>;
fn mint_installation(txn: StoreTxn, id: InstallationId, policy: PrivacyPolicy, backup: Option<Vec<u8>>) -> Result<(), Error>;
fn add_installations(txn: StoreTxn, ids: Vec<InstallationId>);
fn installations() -> Vec<(InstallationId, ScanProgress)>;
fn record_scan(txn: StoreTxn, installation: InstallationId, progress: ScanProgress);
fn set_policy(txn: StoreTxn, policy: PrivacyPolicy);
fn policy() -> PrivacyPolicy;
fn next_operation_id(txn: StoreTxn) -> [u8; 32];
fn purse_backup() -> Vec<u8>;

// reads for others
fn balance(view: Snapshot, purse: PurseId, now: Time, params: ChainParams) -> Balance;     // G1's type
fn snapshot() -> Snapshot;
```

An operation that takes neither a `Snapshot` nor a `StoreTxn`, here and in B4 and C1, reads the latest committed state.

- **`create_purse`** allocates the identifier one past the largest product purse identifier this layer knows of: every purse it has created (closed ones included), those in a restored metadata backup, and any that a reconstruction found. It records, in the same commit as the purse, the name, the creation time and `tag`, with state `Open`. The `tag` is opaque bytes that this layer stores and returns but never reads. The caller uses it to attach something this layer has no concept of, namely the product that created the purse.
- **`purse`** / **`purses`** return stored metadata and state. G1's `query_purse` and `list_purses` add the balance and show only purses that are not closed.
- **`begin_delete`** moves an open product purse to `Deleting { drain_into }`. It fails with `MainPurse` for the main purse, `DeletionOpen` if the purse is already `Deleting`, and `PurseNotFound` if the purse, or `drain_into`, is not `Open`, or if `drain_into` is the purse itself. **`close_purse`** moves a `Deleting` purse to `Closed`; G1 calls it once the conditions of G1 invariant 4 hold.
- **`resolve_purse`** returns the purse itself while it is `Open`, and otherwise follows `drain_into` until it reaches an open purse (invariant 7). Every component that places value in a purse resolves it first: K-flows of layer 2, reclaims and refunds.
- **`allocate`** records one fresh asset per output in `purse` under the current installation, at the next indices, with the output's exponent, and returns them in order (invariant 4). E3 calls it in step 2. **`adopt`** records assets a reconstruction found, under their own installation, without advancing any counter.
- **`keep_secrets`** stores the secrets of the peer coins an import claims; E3 calls it in the import's registration (step 2). **`secret`** returns the stored secret of a peer coin's account, which C2's `prepare` passes to E2 to sign the claim. **`forget_secrets`** deletes them; F2 calls it once each claim is final (G1 invariant 5).
- **`asset`** / **`assets`** read asset identities. For the main purse, `assets` also returns every asset under an earlier installation, whatever purse identifier it was found under (invariant 5).
- **`observe`** stores what F1 reported. For each read it sets the third axis of B1: a value read at a block not older than the stored one replaces the stored value and makes the asset *observed at* that block; a `Failed` read, or a read at an older block, leaves the stored value and makes the asset *read failed since* the stored block (invariant 6). The first time an entry's key is seen included in its ring's root, its inclusion instant is set to `now`. A `Token` read is stored for the token identity, and D1's `token_state` reads it.
- **`mark_disclosed`** writes uncommitted marks. For a coin an export hands out as it is, which no registration holds, it refuses a coin that is already marked or held, in the same store transaction (invariant 2). **`commit_disclosed`** makes marks committed. **`release_disclosed`** deletes uncommitted marks and leaves committed ones; G1's `release_export` and G3's start sequence (G3 invariant 5, using `uncommitted`) call it.
- **`mint_installation`** records a fresh installation as current, stores the initial policy, and restores the purse backup if one is given, all in `txn` (G3 invariant 3). It fails with `UndecodableStore` if the backup does not decode. **`add_installations`** records earlier installations a reconstruction learned (F4); **`record_scan`** stores F4's scan progress for an installation.
- **`set_policy`** replaces the stored policy; nothing stored changes with it (invariant 1). **`next_operation_id`** returns the next id from the counter for the layer's own operations.
- **`purse_backup`** returns `SCALE(Vec<PurseRecord>)` of every purse ever created, closed ones included, so that a restored backup keeps both the owners and the drain targets, and allocation continues past every identifier.
- **`balance`** computes the three-way partition below from `view` at `now` with `params`. It takes each asset's bucket from B1's `selectability`, and counts an entry as *spendable* only while D1's supply for the current period (D1 `supply`, from the same snapshot) can give it a free token. Tokens are assigned across purses in one fixed order: the main purse first, then purses by identifier.
- **`snapshot`** returns a consistent read-only view (invariant 3).

**Invariants**
1. **Nothing here duplicates a fact owned elsewhere.** B2 stores no asset status: no reservation flag and no spent flag. Conflicts between operations are excluded by C1's registration check, inside the same store transaction, instead of by keeping two copies of a status consistent. B2 does not store a classification either. Classifications are recomputed from the current records, because they depend on the current balance ratio and on the ages of other holdings. So a change of policy changes the classifications and requires no change to any stored record.
2. **A disclosure mark has two phases.** It is written *uncommitted* when an export reserves its coins. It becomes *committed* inside the store transaction in which the caller makes the secrets durable (G1 invariant 3). An export may hand out a coin as it is, with no transaction, and then no registration covers the coin. For such a coin, the mark is checked and written in one store transaction, so no other operation can select the coin in between. Every uncommitted mark is deleted at start (G3 invariant 5). A committed mark is never deleted.
3. The ledger serves a **consistent read-only snapshot** to a reader that is not the code changing it, because the durability layer's completion oracle needs one.
4. Allocation is monotonic per `(purse, installation)`, never reissues an index, and happens only in the current installation (A2). Every field that records what has been used is **append-only**: a known installation is never removed, a counter never decreases, and a committed mark is never removed.
5. **A purse holds only the coins and entries under the current installation.** Everything under an earlier installation counts toward the main purse, whatever purse identifier it was found under: the main purse's balance includes it, and the main purse's selection may spend it. This also applies to a purse restored from the metadata backup: the purse comes back with its owner, but without the value it held before. So a purse identifier that appears again after a reinstall never gives a caller value from before the reinstall. A reinstall without a usable backup can at most lose the record of which caller a purse belonged to. It never loses the assets, and it never gives them to the wrong caller.
6. **A failed read never overwrites a stored value.** This one rule covers two failures that look unrelated. A lagging read must not remove a coin from a balance by reading it as zero. And a failed read of an unloaded count must not be taken as "nothing unloaded", which would overstate privacy. A reading at an older block than the block a stored value rests on counts as failed, so a lagging node never moves an observation back in time.
7. **A purse's state only moves forward**, from `Open` to `Deleting` to `Closed`, and a purse is never reopened. A deletion's target is fixed when the deletion begins and is `Open` at that moment. So following targets never returns to a purse already passed, and `resolve_purse` always ends at an open purse.

**Balance** is a **three-way partition**: every holding is in exactly one bucket, and the three buckets sum to the purse total.

| Bucket | Contents |
|---|---|
| **Spendable** | Selectable now (B1 `Now`), under the effective policy (A4). An entry counts here only while this period's free tokens (D1) can unload it, at one token per unload transaction (E1 invariant 4) |
| **Held back** | B1 `HeldBack`: unavailable on purpose, to gain privacy: an entry inside its dwell, or in a ring below every rule's member count, and a coin the recycling sweep takes (B1 `recycle_due`) |
| **Arriving** | B1 `Arriving`, and entries otherwise selectable now that have no token this period: unavailable, but not by the user's choice |

A holding that fits both *held back* and *arriving* is *arriving*. Tokens are counted only against entries that are otherwise selectable now. An asset held as an input (`InUse`) is in no bucket. Instead, the operation holding it counts through its outputs that land in this purse, and those outputs are *arriving* until they are stable. So a spend lowers the total at once, and a move or a payment's change is never counted twice. Assets B1 classifies `Never` are in no bucket.

A coin the recycling sweep takes is **held back** from the moment B1's `recycle_due` returns it until the recycling holds it as an input; F2 starts that recycling at its next call. A coin old enough to recycle that the cap does not admit stays spendable. A coin at the age cutoff is *arriving*: no choice makes it spendable, and its value comes back as entries once it is recycled.

The balance is a partition rather than several overlapping sums, because which sum would apply depends on a policy the caller cannot see. The important distinction is between **held back and arriving**. Held back is value the user could have now by choosing a different policy. Arriving is value that no choice makes available sooner. A chain-locked coin is *arriving*: it becomes available without user action, and no setting shortens the wait.

**The balance never waits on a chain read.** It is computed from stored state, and any input that would need a fresh read uses the last stored value (B2 invariant 6). Otherwise a lagging connection would delay the whole figure while one threshold is evaluated. A spend must lower the displayed number at once, whatever the connection is doing. Only the decisions about what may be selected wait for chain reads.

**Verified by** invariant checks after every operation sequence: no index reissued, no committed mark removed, no purse state moved backwards, balance equal to a direct projection over the facts, and a snapshot taken mid-mutation reflecting exactly one committed state.

---

#### B3 · Selection

**Owns** nothing. **Band:** pure + clock.

B3 decides which assets an operation should consume to produce a requested amount.

```rust
enum Want { Amount(Amount), Everything }        // Everything: a drain (G1 invariant 4)
enum Forms { Any, EntriesOnly }                  // EntriesOnly: an offload phase (F3)
struct SelectRequest {
    purse: PurseId, want: Want, forms: Forms,
    entries_first: bool,                         // a spend layer 2 makes for a product (invariant 2)
    held_back: bool,                             // held-back inputs count as usable now (A4)
}

struct Selection {
    purse: PurseId,
    inputs: Vec<Asset>,                          // in the order chosen
    splits: Vec<(Asset, Vec<Exponent>)>,         // a coin to split, and the exponents it splits into (A5)
    outputs: Vec<Exponent>,                      // the denominations delivered to the destination
    depends_on: Vec<RecordId>,                   // the records still minting an input (C1 invariant 5)
}
enum SelectError { InsufficientFunds { available: Amount }, NoUnloadToken }

fn select(view: Snapshot, request: SelectRequest, now: Time, params: ChainParams) -> Result<Selection, SelectError>;
```

- **`select`** chooses inputs from `request.purse` (resolved by the caller with B2's `resolve_purse`), using each asset's classification (B1) and D1's token supply, both from `view` with `params`, and A5's split plan with `params`. It returns the inputs, the splits to perform, the denominations of the value delivered, and the records it depends on. The value of `inputs` covers `outputs` plus whatever the splits and unload groups keep as change, which E1 places back in the purse. It fails with `InsufficientFunds` or `NoUnloadToken` only as invariant 4 says. `Everything` takes every asset the purse holds that is not `Never`, coins still being minted into it as dependencies, and fails with nothing. Export, rebalance and payments pass `Amount` and `Any`; a drain passes `Everything` and `Any`; an offload's offboarding phase passes `EntriesOnly`. The sweeps (F2) and an offload's recycle phase choose their inputs themselves and do not call `select`. `entries_first` and `held_back` are off except where E3 and F3 set them.

Selection may take inputs that are not usable yet but will become usable without anyone deciding anything:
- coins still being minted by another of this layer's operations, taken as a dependency (C1 invariant 5), if their age as minted is below the age cutoff. Never a coin that an uncommitted export mints, because its build waits on the export's caller (C2 invariant 5);
- coins inside a chain lock;
- entries inside their dwell, whose unload then presents a token of the period in which that wait ends. Never an entry whose ring is still filling, because nobody knows when that wait ends. And never an entry whose ring will be within its rescue margin before that wait ends or before its token's period ends.

The operation registers such inputs at once, which holds them (B1 invariant 2), and its transactions wait to be built until the inputs are usable (B4 `Waiting`). So a payment from value that is still settling starts, and its secrets are released, instead of being refused.

With `held_back`, every held-back input (B1 `HeldBack`) counts as usable now: an entry inside its dwell, an entry whose ring has not reached any rule's member count, and a coin the recycling sweep would take. E3 and F3 set it only as the policy allows (A4).

Selection tries three strategies, first over the inputs usable now and then over all inputs (invariant 5). Each time it tries them in a fixed order, and the first that succeeds is used: with `entries_first`, unload entries first, then spend coins that already sum to the amount, then split coins; otherwise coins that already sum, then split coins, then unload entries. The three strategies are: spend coins that already sum to the amount; reshape coins by splitting them (A5); unload recycler entries, which costs one unload token per unload transaction (E1 invariant 4). When neither coins nor entries alone cover the amount, the last strategy takes both.

**Invariants**
1. **Selection reads a classification, never a policy.** Readiness is defined by B1 and computed at the time given to selection, so an entry's state already includes A4's thresholds. A chain lock is likewise already part of selectability. Selection applies no threshold of its own.
2. **The order of the strategies is fixed by who asked for the spend**, as in the shipped wallet (§5), and the policy does not set it (A4 invariant 2). A spend layer 2 makes for a product takes entries first, because spending an entry that is already private is better than creating one; every other spend takes coins first. After invariant 5's preference, this order is the sort key. Selection's own tie-breaks come after it and decide every remaining tie, so two implementations with the same records and the same request choose the same records. This is required for conformance, not for performance.
3. Selection does **not** decide how to reshape coins. It establishes that the coins cover the amount, hands the per-exponent counts to A5, and applies A5's plan exactly as returned (A5, What it relies on). It chooses which individual coin of each exponent is consumed, using its own ordering.
4. **Selection fails only for lack of value or of unload tokens.** It fails when the purse's value does not cover the request, counting everything the purse may take now or later. It also fails when only entries cover the request and D1 cannot promise their tokens (D1 invariant 1). No request fails because of the denominations of its coins. Every operation accepts any coins that sum to its amount, and a split can always produce such coins from larger ones (A5's precondition).
5. **Usable now comes before usable later.** Among inputs that would cover the request, selection prefers those usable at once. This preference applies before the policy's spend order and before the tie-breaks of invariant 2. A payment waits only when nothing usable now covers it. With `held_back`, held-back inputs are usable now.

**Verified by** the determinism property, by exhaustively checking which purses fail over generated purses, and by checking a returned plan's outputs sum to the request.

---

#### B4 · Operation model

**Owns** each operation's identity and kind, the arguments it was started with, a requested cancellation, an export's release, and an offload's stored wait and end (F3 invariant 4). Everything else about an operation is read from its transactions' records. **Band:** stateful, effect-free.

```rust
enum OpKind { Rebalance, Delete, TopUp, Offload, Export, Reclaim, Import, Reconstruct,
              Rescue, Recycle, RegisterInstallation }
enum OffloadEnd { Done, Failed, Cancelled }

fn begin(txn: StoreTxn, key: OpKey, kind: OpKind, arguments: Vec<u8>) -> Result<Started, Error>;
fn request_cancel(txn: StoreTxn, key: OpKey) -> Result<(), Error>;
fn cancel_requested(key: OpKey) -> bool;
fn record_release(txn: StoreTxn, export: OpKey);
fn record_offload_end(txn: StoreTxn, offload: OpKey, end: OffloadEnd);
fn record_offload_wait(txn: StoreTxn, offload: OpKey, until: Option<Time>);   // None: until the next run_due
fn offload_wait(key: OpKey) -> Option<Option<Time>>;                          // the stored wait, if the offload is waiting
fn status(view: Snapshot, key: OpKey, now: Time) -> Result<StatusItem, Error>;
fn completion(view: Snapshot, key: OpKey) -> Option<Completion>;
fn open_operations(view: Snapshot) -> Vec<OpKey>;
```

- **`begin`** is the first step of every G1 primitive that starts an operation. `arguments` is the SCALE encoding of the primitive's arguments. If `key` is new, it records the operation and returns `New`. If `key` names an operation with the same kind and arguments, it returns `Joined` while that operation is open, and `Finished(completion)` once it has ended (invariant 5). If the kind or arguments differ, it fails with `KeyReused`.
- **`request_cancel`** records a cancellation request for an open operation (invariant 1). It fails with `NotCancellable` for an export and for the layer's own operations, and with `UnknownOperation` for an unknown key. A request for an operation that has already ended changes nothing and succeeds. **`cancel_requested`** is what C2's `prepare` reads.
- **`record_release`** records that an export was released (G1 `release_export`) or given up at start (G3 invariant 5); its status becomes `Failed(NotCommitted)`. **`record_offload_wait`** stores the wait F3's Plan decides, and the next Plan clears it; **`offload_wait`** is what F2 reads to know when the next Plan is due. **`record_offload_end`** stores an offload's end when F3's Plan decides it.
- **`status`** computes the operation's current status and value progress from `view`: its records' statuses, C1's rows, B2's observation (for an import's observed outputs), the disclosure marks (for an export's commit) and B4's own stored facts, using the table and rules below. **`completion`** returns the completion once the status is terminal, and none before. **`open_operations`** lists operations whose status is not terminal; F2 resumes the layer's own from it.

**The caller chooses the identity.** An operation is named by a 32-byte identifier that the caller chooses and stores before calling. Operations are keyed by `(scope, id)`. The **scope** is an opaque value the caller supplies to keep its own callers apart (the layer above passes the product), and this layer gives it no meaning. The layer's own operations use the empty scope, which G1 refuses from a caller. Because a repeated call joins the existing operation instead of starting a second one, a caller can safely retry after it crashed between deciding to act and calling.

**An operation is a group of the durability layer's records**, registered under its identity. Its status is computed from the records' statuses, by the first row that applies:

| Records | Status |
|---|---|
| All stable, every one succeeded finally | `Done` |
| All stable, some succeeded finally | `PartiallyDone` |
| All stable, none succeeded | `Failed` |
| Some succeeded, on the best chain or finally | `Settling` |
| Some submitted | `Submitted` |
| Some waiting to be built, the inputs of one of them usable | `Preparing` |
| Otherwise: waiting to be built, no inputs yet usable (B3) | `Waiting` |

`Waiting` names no time. When a wait ends depends on rings, token periods and other operations, and the layer promises nothing about it. A load's input is its funding account's funds, which the status does not read, and C2 builds a load as soon as they cover it. So a load waiting to be built has no usable input, and a top-up waiting for its funds is `Waiting`. A record that failed while its rebuild policy allows another attempt is back to *waiting to be built*, so the operation returns to `Preparing` or `Waiting`.

For three kinds of operation, the status depends on more than the records.

An **import or reclaim** is judged per coin. It is `Done` when every coin moved (for a reclaim: moved, or was never minted), `PartiallyDone` when some did, and `Failed(NothingMoved)` when none did. A coin moved by an import settles at the exponent observed in its output account, and an import ends only once every moved coin's output has been observed. A reclaim ends only once each coin has a result. Until then it is `Waiting`, or `Submitted` while the minting it waits for has an attempt in flight.

An operation with **no transaction of its own**, such as an export of coins that already have the right denominations, or a reconstruction, is `Preparing` until its own step is done, and then `Done`. An export's own step is its commit, and a released export is `Failed(NotCommitted)`. An export that also mints coins counts the coins it hands out unchanged as settled at its commit, so a minting that fails after the commit ends the export `PartiallyDone`.

An **offload** is `Waiting` between its phases (F3) until its stored end decides its terminal status, by the value offboarded. Its progress counts only the value offboarded.

**Identity never expires.** Records are never deleted (C1 invariant 1), so an operation's outcome stays readable for as long as the store exists. The outcome consists of its terminal status, its amount, and, for every transaction that succeeded, its hash, its finalized block, and the coin accounts it consumed and created. This outcome is the operation's **completion** (G1 `Completion`). A caller needs it to point at the payment on chain later (RFC-17's clearing reference). The completion is as sensitive as C1's rows (C1 invariant 6), and the layer above decides what of it reaches a product. A completion never names a recycler entry, because naming which entry an unload consumed would reveal the link that the ring hides (P7). For the same reason, an offload's completion lists only its offboarding transactions: its recycling transactions would link the coins it recycled to the destination of the value.

**Invariants**
1. **Cancellation is a request**, accepted until the operation ends. There are two exceptions. An export is ended by `release_export` before its commit and by `reclaim` after it. And the operations the layer starts itself under its reserved scope cannot be cancelled by any caller (`NotCancellable`). After a cancellation, `prepare` gives up every record of the operation at its next build (C2 invariant 5), and an attempt already in flight ends on its own status (E3 invariant 3). So a cancelled operation ends `Failed(Cancelled)`, or `PartiallyDone` if a transaction it had already submitted succeeds.
2. `Settling` is **provisional and never final**. It may be used for display.
3. `Done` means every transaction succeeded finally. `PartiallyDone` means some did and the rest failed. Partial success is a normal outcome, not an error. **`PartiallyDone(amount)`** is a status, not only a detail of the receipt. Its amount is read back from the records rather than accumulated in memory, so a caller that reconnects does not have to work out how far the operation got.
4. A receipt records only outcomes that reached a **stable** verdict, and its block references are always finalized ones.
5. **Calling again with the pair of a completed operation returns its completion. It never starts a second operation**, however late the call is. A caller that crashed while its payment was settling finds the operation and joins it, instead of starting it again and paying twice.
6. **Every status carries the operation's value progress**: the value already settled on a stable verdict, the value still in flight, and, of the value in flight, the part whose transaction succeeded on the best chain but not yet finally, all read back from the records. An operation of many transactions settles in steps. A caller that reports progress, as RFC-17's `Clearing { clearing, cleared }` does, needs the first two numbers at every step, not only at the end. A caller that reports a provisional outcome, as RFC-6's `Claimed { finalized: false }` does, needs the third: one transaction that succeeded on the best chain does not mean the whole amount did (invariant 2).

**Verified by** checking every combination of record statuses against the derived status, that a repeated `begin` with the same arguments joins and with different arguments is refused, and that cancellation is accepted until the terminal status for every operation but an export and the layer's own.

---

### Band C — The durability boundary
*Where the coinage layer registers its transactions with the durability layer, and what it supplies for the durability layer to call back.*

---

#### C1 · Registration

**Owns** the rows that tie each of the durability layer's records to this layer's assets: the inputs the record's transaction consumes, the outputs it creates, the operation it belongs to, the unload tokens it presents, and the records it depends on. **Band:** stateful, effect-free.

These rows are the only place where an asset's use is recorded: B1 computes *in use* and *spent* from these rows and from the record's status. E3 writes them in the registration callback of the durability layer's registration (§1), and C1 checks them there.

```rust
type InstallationId = [u8; 32];     // A2
type RecordId       = i64;          // the durability layer's DurableTxId, which it assigns when it inserts the record (§1)

enum Asset {                        // what a row names: an index, never an account (invariant 6)
    Coin  { purse: PurseId, installation: InstallationId, index: u32 },
    Entry { purse: PurseId, installation: InstallationId, index: u32 },
    PeerCoin(AccountId),            // a coin a peer sent; only ever an input
}
struct TokenId { period: u32, slot: u32 }     // one unload-token identity (D1)

struct Rows {                       // the rows of one record
    inputs: Vec<Asset>,
    outputs: Vec<Asset>,            // never a PeerCoin
    tokens: Vec<TokenId>,
    depends_on: Vec<RecordId>,      // the records that mint an input of this one (invariant 5)
}
enum Admission { Ordinary, Import, Reclaim }   // which disclosed inputs may be taken (invariant 4)

enum RegisterError {
    InputTaken(Asset),              // invariant 2
    TokenTaken(TokenId),            // invariant 2
    OutputNotFresh(Asset),          // invariant 3
    Disclosed(Asset),               // invariant 4
}

fn register(txn: StoreTxn, op: OpKey, admission: Admission,
            records: Vec<(RecordId, Rows)>) -> Result<(), RegisterError>;
fn rows(record: RecordId) -> Rows;
fn operation(record: RecordId) -> OpKey;
fn consumers(asset: Asset) -> Vec<RecordId>;     // every record that has the asset as an input
fn creator(asset: Asset) -> Option<RecordId>;    // the one record that creates it (invariant 3)
fn presenters(token: TokenId) -> Vec<RecordId>;  // every record that presents the token
```

- **`register`** runs in the registration callback (§1), inside the durability layer's registration transaction `txn`, after the durability layer has inserted the records; `records` pairs each record's identity with its rows, in the order E3 planned them. It checks every row of every record against the rows already stored and against B2's disclosure marks, in the same store transaction. If every check passes, it writes all the rows. If any check fails, it writes nothing and returns the first failure; the durability layer then rolls back the whole registration, records included (invariant 1). For `Admission::Import` and `Admission::Reclaim`, a disclosed input is accepted as invariant 4 describes. A reclaim's destination purse must be open, which G1 checks before registration (G1 invariant 4).
- **`rows`**, **`operation`**, **`consumers`**, **`creator`** and **`presenters`** read committed rows. B1 uses `consumers` and `creator` to classify an asset, C2 and E2 use `rows` to judge and build a record, and D1 uses `presenters` to compute a token's state.

**Invariants**
1. **Rows and record commit together.** A record without its rows would be a transaction whose effects nothing local expects. Rows without a record would hold assets for no transaction. So registration writes both or neither. Neither is ever deleted, not even after the asset is spent, because whether the asset is spent is read from them. B2's marks and counters are likewise append-only (B2 invariant 4).
2. **An asset, or an unload-token identity, is an input of at most one transaction that has not failed.** This is the layer's mutual exclusion. It is checked at registration, in the same store transaction, so two operations cannot claim the same coin, entry or token, even across a restart. Tokens use the same mechanism as inputs on purpose: a token is consumed when presented, exactly as an input is, and a second mechanism would be a second place where the exclusion could be implemented wrongly.
3. **Every output is fresh**, and names exactly one transaction that creates it. An output that is already recorded is refused, and so is a key that a peer sent. Because outputs are fresh, an output that later exists on chain can be attributed to exactly one transaction, and that is what allows completion to be judged from chain state.
4. An asset whose secret may have been disclosed carries a **disclosure mark** (B2 invariant 2), and is never registered as an ordinary input again. Whoever holds the secret controls the asset, so the layer must stop treating the value as its own. Two kinds of registration are exceptions. An **import** of a coin this layer exported, which is a payment between two callers of one user agent, claims the coin as any payee would. A **reclaim** moves a marked coin that is still in its account into a fresh account in the purse the caller names, and that purse must be open. Whichever side moves the coin first owns it.
5. Every input minted by another of this layer's transactions records a dependency on that transaction. The dependency graph is **acyclic by construction**, because the rows of the minting transaction exist before its output can be selected.
6. The rows store **purse-scoped indices, never account identifiers or member keys**. The exception is a coin a peer sent, which has no index here. Identities can be derived from indices, so the rows do not spell out which inputs produced which outputs. That link between inputs and outputs is what the recycler's anonymity set is meant to hide. But the durability layer keeps each attempt's hash next to the rows, and the hash leads to the transaction on chain and the accounts it names. So the store's encryption at rest (store contract, item 5) is what protects the link. The rows are therefore more sensitive than the asset records alone.

**Verified by** attempting every registration the invariants forbid and checking each is refused with nothing written, and by interrupting registration and checking that either everything or nothing was committed.

---

#### C2 · Durability plug-ins

**Owns** nothing; it is what the durability layer calls into. **Band:** pure decision with an effectful half.

C2 defines the layer's two domains, `coinage` and `coinage-registry` (§1), and is the completion oracle and the rebuild policy of each. The core adds both domains to the durability layer's list through `with_coinage` before it creates the durability layer, which is before this layer starts (§1). The durability layer calls the plug-ins; nothing in the coinage layer does.

**Domain definitions.**

```rust
struct Running;               // what G1 and G3 hold in memory, shared with the plug-ins (below)

struct CoinageDeps {
    store: Store,             // the shared store (§1)
    seed: Seed,               // derives every key the plug-ins sign with (A2)
    clock: Clock,
    people: Chain,            // state reads and heads of the People chain, which runs the coinage pallet
    people_genesis: [u8; 32],
    asset_hub: Chain,         // state reads and heads of Asset Hub, which runs the installation registry
    asset_hub_genesis: [u8; 32],
    config: ChainConfig,      // A3: the instance served and the limits the runtime does not publish
    running: Running,
}

fn with_coinage(list: DurableRegistry, deps: CoinageDeps) -> DurableRegistry;
```

- **`with_coinage`** returns `list` with both domains added: `coinage`, on the chain whose genesis is `people_genesis`, and `coinage-registry`, on the chain whose genesis is `asset_hub_genesis`, each with the completion oracle and the rebuild policy below. The two names are constants of C2 and never change (§1).
- **`Running`** is the one piece of in-memory state the plug-ins share with the layer. The core creates it closed and gives the same handle to `with_coinage` and to G3's `start`. G3 opens it at the end of its start sequence (G3 invariant 5). Once open, it gives the `ChainParams` of the last runtime check that passed, whether the latest check failed (G3), and the `Funding` capability of each top-up that has not ended (G1 invariant 6). G1 and G3 write it; the plug-ins only read it. While it is closed, `prepare` answers no record (invariant 6).

`CoinageDeps` holds only what exists before this layer starts. It holds no `ChainParams`, because they exist only after G3's runtime check; everything in C2 that needs them is in `prepare`, behind the gate.

**The plug-ins.**

```rust
enum HeadKind { Finalized, Best }                            // which of a pass's two heads a question is about
enum AttemptFailure { Expired, DispatchFailed, Rejected }    // a failure about to be recorded (§1)

struct Window { birth: u32, period: u32 }    // valid from block `birth` up to, but not including, block `birth + period` (§1)
struct Submitted { record: RecordId, window: Window, params: Vec<u8> }   // a record with an attempt; params: its call and retry policy (E3)
struct Waiting { record: RecordId, params: Vec<u8> }                     // a record waiting to be built

enum Build {
    Ready { bytes: Vec<u8>, birth: BlockRef, period: u32 },   // the window encoded in the bytes (E2 invariant 3)
    GiveUp,                                                   // fails the record (invariant 5)
}

// the completion oracle
fn open_pass(records: Vec<Submitted>, finalized: BlockRef, best: BlockRef) -> Option<Pass>;   // None: the snapshot could not be read
fn proven_completed(pass: &Pass, record: RecordId, head: HeadKind) -> bool;
fn proven_not_completed(pass: &Pass, record: RecordId, head: HeadKind) -> bool;
// the rebuild policy
fn can_retry(record: Submitted, failure: AttemptFailure) -> bool;
fn prepare(batch: Vec<Waiting>) -> Vec<(RecordId, Build)>;   // a record without an answer keeps waiting (invariant 5)
```

- **`open_pass`** is called once per recovery pass for a domain, with the domain's submitted records that the durability layer is deciding in that pass, and the pass's finalized and best heads (§1). It makes every read the answers need, at both heads, and returns a `Pass` that holds the results. It returns none only when it cannot read the snapshot; a chain read that fails gives an unknown result, not a failed pass (invariant 2).
- **`proven_completed`** and **`proven_not_completed`** answer from what `open_pass` read, without reading anything more. Their rules for each domain are below.
- **`can_retry`** answers from the failure and the record's retry policy (E3) alone (invariant 3).
- **`prepare`** returns, for each record of the batch it can decide, either the next attempt's bytes with the birth block and period encoded in them, or `GiveUp`; a record it does not answer keeps waiting (invariant 5). It rebuilds the planned transaction from C1's rows and the call in the record's policy parameters, and builds it through E2: it reads E2's context at a finalized block, chooses the window, and assembles. The period is A3's longest validity window, halved until the window ends before the retry policy's deadline when the policy is `Until`. The birth block is the finalized block the context is read at; for a period above 4096, it is the latest block at or before that one which the durability layer's rule on birth blocks allows (§1).

The effectful half of each plug-in is its reads: `open_pass` reads one snapshot and the chain at the pass's two heads; `prepare` reads one snapshot, the clock, the `ChainParams` and a top-up's `Funding` capability through `Running`, and E2's context. Each answer is a pure function of what was read. `can_retry` reads nothing (invariant 3). The plug-ins read the clock themselves, because deadlines, margins and periods need it.

**The `coinage` oracle.** For each record, `open_pass` reads from one snapshot (B2 invariant 3) the record's rows (C1), the disclosure marks on its inputs and outputs (B2), every record that has one of its outputs as an input (C1 `consumers`), every record it depends on, and the statuses of all of these (§1). From the chain, at each head, it reads: for each coin among the inputs and outputs, the coin at its account and, for an input, the lock on that account; for each entry, its ring membership and, for an input whose ring position F1 has observed, the state of its alias; and, for a record that presents an unload token, whether the chain records the token as consumed and whether the token's period is still accepted. Each chain read is *present* with a value, *absent*, or *unknown*. A coin present with a value other than the one its asset record gives, or an entry in the ring of another exponent, is read as unknown and logged.

These terms apply to one record at one head:
- An input is **available** when it is a coin present at its account, or an entry whose membership is present and whose alias is not unloaded. A coin a peer sent and an input with a disclosure mark are never available, because someone else holds their key and could have spent them and sent another coin to the same account.
- An output is **untouched** when it has no disclosure mark and no record that has not failed has it as an input, records waiting to be built included.
- An input is **settled** when it is a coin this layer minted (C1 `creator`), with no disclosure mark, and the record that minted it has succeeded finally and its window is closed at the finalized head.
- **No failed dispatch shows** when: for a coin origin, no input coin is locked; for a token origin, the chain does not record the token as consumed and the token's period is still accepted. An unpaid signed load needs no check, because its dispatch cannot fail in a valid block (§1).

`proven_completed` holds when any of these holds at the head:
1. an output of the record is present (only this record creates it, C1 invariant 3);
2. an entry input has its alias unloaded (only a successful unload does that, §1);
3. the record has at least one input, every input is settled, and every input is absent;
4. a record that has one of its outputs as an input has succeeded finally.

`proven_not_completed` holds when no failed dispatch shows at the head, and any of these holds there:
1. an input of the record is available;
2. an untouched output of the record that is a coin is absent;
3. a record it depends on has failed.

Each of the three rests on a fact that cannot reverse. A spent input never returns, because no key is used twice (P5) and nobody else holds the key. An untouched output can be created only by this record, and nothing could have spent it. A failed record created nothing (§1), so an input it would have minted never exists. The condition that no failed dispatch shows keeps the durability layer from recording a failed dispatch as an expiry (invariant 3).

**The `coinage-registry` oracle.** `open_pass` reads, at each head, the records the installation registry returns (F4 invariant 4). `proven_completed` holds when the sealed record the transaction appends is among them. `proven_not_completed` never holds; the durability layer decides an append that did not complete by searching the finalized blocks of its window (§1).

```
waiting to be built ──prepare: bytes──▶ submitted ──▶ succeeded on best chain ──▶ succeeded finally
        ▲                                   │                    │
        │ can_retry: yes                    ▼                    ▼ (withdrawn on reorganisation)
        └───────────────────────────── attempt failed         submitted
                                            │ can_retry: no, or prepare: give up
                                            ▼
                                          failed
```

**Invariants**
1. **A provisional status is used for exactly one thing**: it can make a dependent transaction eligible to be built. This is allowed only when the dependent transaction's own success would prove that the earlier transaction executed on the same chain. Where that implication does not hold, `prepare` waits for a stable status. This is the only place where the layer accepts less certainty in exchange for lower latency, and it is safe only because the implication is checked for each transaction. The oracle never uses a provisional status as evidence: rule 4 of `proven_completed` and rule 3 of `proven_not_completed` read only stable ones.
2. **The oracle claims only what the evidence proves.** Every rule needs each read it uses to be present or absent; an unknown read makes the rule not hold, so a failed read never becomes evidence. *Not completed* rests only on facts that cannot reverse (above). The oracle draws nothing from the absence of an entry or an alias state, because ring cleanup removes them (§1). It reads a token as not consumed only while the token's period is still accepted, because cleanup removes a consumption only after that (§1).
3. **`can_retry` reads no chain and no store.** The durability layer asks it just before it records a failure, and it answers at once from the failure and from the retry policy the record carries (E3). A failure that would happen again on the same inputs is not retried without limit, because nothing else would stop the loop. For a transaction that presents an unload token, it answers *no* to `DispatchFailed`: that attempt reached the chain and spent the token, and every attempt presents the same token (invariant 5). `Expired` and `Rejected` mean no attempt reached the chain: the durability layer records *expired* either from a search of every finalized block of the window or from the oracle's *not completed*, and the oracle claims that only when no failed dispatch shows. A cancellation is handled by `prepare`, which gives the record up at its next build (invariant 5). For a record whose policy parameters do not decode, it answers *no* (E3).
4. **Both plug-ins are read-only and never re-enter the layer.** They never submit, never write this layer's state, and never call back into the layer. They read one committed snapshot per call (B2 invariant 3).
5. **`prepare` builds exactly what was registered.** Every attempt of one transaction consumes the same inputs and creates the same outputs. An attempt that would change either is a different transaction and needs its own registration. `prepare` decides each record from the record itself, the store and the chain, never from which other records the batch holds, because the batch need not hold every record that is waiting (§1). It may wait for inputs to become usable. It may also wait for an earlier-registered transaction of the same signing account to reach a stable status, because the layer builds at most one transaction per signing account at a time, in registration order (E2 invariant 4). It does not answer an export that is not yet committed, so no minting is built before its handoff is committed (G1 invariant 3). It also does not answer a top-up whose funding account cannot cover it yet, nor any record while the current runtime fails A3's check (G3 invariant 7). Not answering is not a verdict: the record stays *waiting to be built*. `prepare` gives up on a record in these cases: its policy parameters do not decode (E3); its inputs, or a top-up's funds, are gone past the operation's deadline; its operation's cancellation was requested (B4 invariant 1); the record mints an export that was released, or that was not committed before a restart (G3 invariant 5); the record mints a coin that a reclaim names, in which case the coin is never minted and the record's inputs are freed; a token it presents belongs to a period that has ended; it needs a funding signature that the signer declines or that the layer no longer has (G1 `Funding`, invariant 6); and, for an ordinary record, an input entry's ring comes within the rescue margin (F2 invariant 1), in which case giving up frees the entry for the rescue sweep.
6. **Building waits for the start sequence.** While `Running` is closed, that is, until G3 has run its start sequence (G3 invariant 5), `prepare` answers no record. The durability layer can call the plug-ins from the moment the core creates it, which is before this layer has started (§1). Without the gate, an export that the last run left uncommitted could be minted before G3 gives it up. The oracle and `can_retry` need no gate: they build nothing and write nothing, and a disclosure mark that G3 has not yet deleted only removes evidence.

**Not here:** how a status is reached. Submission, following an attempt, recovery passes, reorganisation handling and the search over blocks belong to the durability layer. Also not handled: a coin-origin attempt whose dispatch failed and whose same bytes are included again after the coin's lock has expired, inside their window (§1). The durability layer records a failed dispatch read in a finalized block as final at once, so such a later success is not recorded (§5).

**Verified by** driving every status sequence, including a provisional success later withdrawn, against the oracle's answers at both heads; by checking each *not completed* rule against chains on which the transaction did complete and its outputs were spent afterwards, its entries' ring was cleaned up, or a token-origin dispatch failed; by checking that `prepare` never returns bytes whose inputs or outputs differ from the registration, nor a window different from the one encoded in the bytes; and by checking that `prepare` answers no record before G3's start sequence has run.

---

### Band D — Resources
*A pure decision over the unload-token supply.*

---

#### D1 · Unload-token supply

**Owns** nothing; it decides over the token state that C1's rows, the records' statuses and F1's observation give. **Band:** pure.

Unloading a recycler entry costs one **unload token**. The layer uses only the free tokens of the user's personhood credential (§6). One key produces a fresh token identity per counter value, so a single key covers a whole period's allowance. Every wallet holds such a credential. A **period** is the chain's accounting window for the allowance, and a **slot** is one counter value within a period, that is, one token identity.

An unload that presents a token is **feeless**: it declares a fee ceiling of zero, and no account pays anything. So the layer holds no fee account, and nothing needs funding from outside.

```rust
enum TokenState { Applied, NotApplied, Unknown }

struct Supply {
    period: u32,
    allowance: u32,                      // tokens per period (A3)
    reserve: u32,                        // invariant 2; zero for a small allowance
    states: Vec<TokenState>,             // one per slot of the period, by slot
    held: Vec<bool>,                     // one per slot: an input of a transaction that has not failed (C1 invariant 2)
}

fn token_state(token: TokenId, view: Snapshot) -> TokenState;
fn supply(now: Time, view: Snapshot, params: ChainParams) -> Supply;
fn resolve(needed: u32, supply: Supply) -> Option<Vec<TokenId>>;   // None: short
fn running_low(supply: Supply) -> bool;
```

- **`token_state`** computes a token's application state, defined below, from the records that present it (C1 `presenters`), their statuses, and F1's observation in the snapshot.
- **`supply`** gives the current period's supply: the period that contains `now`, the allowance from A3, the reserve (invariant 2), and every slot's state.
- **`resolve`** returns `needed` token identities, or none if the supply is short (invariant 1). A slot is free when it is `NotApplied` and not held. It takes free slots lowest first (invariant 3).
- **`running_low`** holds when the number of free slots is at most the reserve (invariant 2).

E3 calls `resolve` once per operation. B3 calls `supply` and `resolve` to decide whether entries can be promised their tokens (B3 invariant 4), and B2's balance uses them to count only entries that this period's tokens can unload. B1's `recycle_due` calls `supply` and `running_low`. G1 reports a short supply as `NoUnloadToken`.

**Invariants**
1. A plan that is short of even one token is **refused whole**. A partial plan would spend its tokens and then fail anyway.
2. **The reserve only stops the recycling the policy asks for.** Every operation may take any free slot. While `running_low` holds, B1's `recycle_due` admits no coin, so the layer does not turn coins into entries that the remaining tokens may not be able to unload. Recycling a coin at the age cutoff is not stopped (F2 invariant 6). The reserve is the allowance times a per-network share (A3), rounded down: the shipped wallet's share is 20 percent, so an allowance below five has a reserve of zero. The reserve is outside the policy, so no setting changes it. No token is kept for the rescue sweep: it takes free slots like every other operation, and a rescue that finds none waits for the next period (P10).
3. **Slot resolution is deterministic**: lowest index first. This is needed for conformance, because which token a payment spends is visible on chain. Without it, two implementations could satisfy every other invariant and still spend different tokens.

**A token identity's application state** is one of three values. It is `applied` once some operation using it has reached the chain, whether or not that dispatch succeeded, or once F1 observes the token consumed. It is `not applied` once every operation using it has finally failed without reaching the chain. It is `unknown` in between. The observation by F1 covers a token spent outside this store: before a wipe, or by another device on the same credential. The layer reserves only tokens that are `not applied`. **`Unknown` blocks a token exactly as `applied` does**: spending a token that may already be used wastes the allowance and produces a transaction the runtime refuses.

**Verified by** enumerating supply against demand and checking the plan is either complete or absent, never partial; by fixed vectors for the period arithmetic; and by checking no token identity is reserved twice across a restart.

---

### Band E — Execution
*Effects only through the interfaces of §1.*

---

#### E1 · Transaction planning

**Owns** nothing. **Band:** pure. E1 is a function of a ledger snapshot.

E1 turns what an operation is asked to do into an ordered set of transactions: which call each transaction makes (§1), what it consumes, what it creates and where, which token it presents, and which earlier transaction it depends on. It plans every kind of operation.

```rust
enum Destination { Purse(PurseId), Export(PurseId), External(AccountId) }   // Export: fresh accounts in the payer's purse, whose secrets are handed out

enum PlanInput {
    Spend { selection: Selection, to: Destination },   // Selection: B3's result, or the inputs a sweep or an offload phase chose (F2, F3)
    Import  { coins: Vec<CoinSecret>, into: PurseId },  // peer coins, each named by the account its secret controls (C1 Admission::Import)
    Reclaim { coins: Vec<Asset>, into: PurseId },       // marked coins of this layer (C1 Admission::Reclaim)
    Load  { from: AccountId, amount: Amount, into: PurseId },   // a top-up
    Recycle { coins: Vec<Asset>, into: PurseId },       // a recycling sweep (F2) or an offload's recycle phase (F3)
    RegisterInstallation(InstallationId),               // G3 invariant 4
}

enum Call { Split, Transfer, Recycle, UnloadToCoins, UnloadToExternal(AccountId),
            Load(AccountId), AppendRegistryRecord }    // §1's calls
enum Output { Coin { purse: PurseId, exponent: Exponent }, Entry { purse: PurseId, exponent: Exponent } }

struct Planned {
    call: Call,
    inputs: Vec<Asset>,
    outputs: Vec<Output>,       // the caller binds each to a fresh index (E3)
    token: Option<TokenId>,
    depends_on: Vec<RecordId>,
}

fn tokens_needed(input: PlanInput, view: Snapshot, params: ChainParams) -> u32;
fn plan(input: PlanInput, tokens: Vec<TokenId>, view: Snapshot, params: ChainParams) -> Vec<Planned>;
```

- **`tokens_needed`** counts the unload transactions `plan` will produce for this input, which is the number of tokens the operation needs.
- **`plan`** returns the transactions in a fixed order, each with its call, its inputs, the kind, purse and denomination of each output, the token it presents, and the records it depends on. A `Spend` produces the splits of the selection, transfers of coins already in shape, and one unload per unload group; for `Destination::Export`, a coin already in the right denomination is handed out as it is, with no transaction (G1 invariant 5), and every other coin is minted into a fresh account of the payer's own purse. An `Import` or a `Reclaim` produces one transfer per coin into a fresh account of `into`. For `Destination::Purse`, coins move by direct transfer, or by recycling into entries of the destination purse when A4's `move_through_recycler` is on. A `Load` produces the loads that turn `amount` from `from` into entries of `into`. A `Recycle` produces one recycle per coin, into fresh entries of `into`. `RegisterInstallation` produces the one contract call.

**Invariants**
1. **A payment's transactions are independent of one another.** Both split and unload name a destination for each coin they produce, so a payment's coins are minted directly where they end up: into fresh accounts of the payer's own, whose secrets are then exported. A payment never needs to mint a coin and then move it, and when one transaction fails, the others do not depend on it.
2. So a dependency arises only where a later transaction actually spends an earlier transaction's output. In this layer that happens only across operations, when selection takes a coin that another operation is still minting (B3). Within one operation there are no dependencies. A5 guarantees that no split consumes another split's output, so a whole split plan is submitted in one round. And an offload plans each phase only once the previous phase is final (F3 invariant 2).
3. Each unload group is one ring at one revision, and carries its own token. The reason is that a proof built against one ring revision does not verify against another, so grouping entries by ring index alone produces transactions the chain refuses. The group's outputs equal its inputs in value, and the group's own change takes the remainder. **No transaction mints into more than one purse**, except a move between purses, where the user chose to link the two purses (A4).
4. Per-transaction caps are chain limits, and they are enforced here, before anything is derived. An unload group over the cap is split across several transactions, each with its own token, so no transaction is built over a cap.
5. **Planning is deterministic.** Outputs take the allocated indices in order, and an over-cap group is cut at a fixed size. Tokens are resolved once per operation (E3), not once per group, and are assigned to groups lowest slot first (D1 invariant 3). If tokens were resolved per group, two groups could be given the same free slot, and the second group's transaction would be refused after the first had spent the token.

**Verified by** checking value conservation per group, cap compliance, and that dependencies appear exactly where an output is consumed.

---

#### E2 · Transaction assembly

**Owns** nothing. **Band:** pure decision with an effectful half. E2 makes three narrow reads, and the byte-building is pure.

E2 turns one planned transaction into signed bytes: the call, the origin, the proofs, the validity window and the signature. Only C2's `prepare` calls it.

```rust
struct Context {
    ring: Option<Ring>,          // for an unload: the ring holding its entries, at the revision the group uses (§1)
    identity: Option<Vec<u8>>,   // for an unload: the entry identity state its proof needs, as F1 observes it
    nonce: Option<u32>,          // for a signed origin: the signing account's next nonce
}
struct Ring { index: u32, revision: u32, page_size: u32, members: Vec<[u8; 32]> }   // members in ring order

enum Signer { Seed, Imported(CoinSecret), Funding(Funding) }   // where the origin's key comes from

fn read_context(tx: Planned, at: BlockRef) -> Context;
fn assemble(tx: Planned, context: Context, birth: BlockRef, period: u32, signer: Signer) -> Option<Vec<u8>>;   // None: the signer declined
```

- **`read_context`** reads, at the finalized block `at`, only what the bytes need and the layer cannot compute: the ring, the identity state and the nonce (invariant 4).
- **`assemble`** builds the bytes from its arguments alone, so equal arguments give equal bytes. The key comes from the seed for the layer's own coins, entries and the `//datastore` account (A2), from an imported coin secret for a peer's coin (G1 invariant 5), or from the `Funding` capability for a top-up's funding account (G1 invariant 6). It returns none only when `Funding` declines to sign.

**Invariants**
1. Each origin (§1) has exactly one construction path, so the rules specific to that origin are enforced in one place rather than at each use.
2. A token is presented only by the transaction whose registration holds it (C1 invariant 2). Assembly takes nothing from a pool and writes nothing, because it runs inside a plug-in, and plug-ins never write (C2 invariant 4). Whether the token was spent is read from what happened to the attempts (D1).
3. **Every transaction is mortal**, and the validity window that the durability layer records is the window actually encoded in the bytes. Mortality is what bounds the durability layer's search, and what lets it eventually declare an unresolved transaction dead. An immortal transaction never reaches such a point, so its inputs could never safely be freed. If the recorded window differed from the encoded one, the durability layer would be answering a question about a different transaction.
4. Assembly reads only what it cannot compute: the ring holding a key, the current identity state, and the nonce of an account that signs. **The layer owns its nonces.** The nonce of a signing account (a funding account or the `//datastore` account) is read at a finalized block no earlier than the block that the previous transaction's stable status rests on, and `prepare` waits for that status (C2 invariant 5). These values are fetched into an explicit context once per transaction, so the byte-building itself stays pure and can be tested with fixed vectors.

**Verified by** fixed vectors: a planned transaction plus a fixed context and window must produce fixed bytes.

---

#### E3 · Operation start

**Owns** nothing; it takes an operation from request to registration. **Band:** pure decision with an effectful half. The effectful half is the registration's store transaction and the random draws made in it; selection, token resolution and planning are pure functions of the state read in that store transaction (B3, D1, E1).

Every primitive of G1 that starts transactions does so through E3, after B4 has given the operation its identity. Building attempts afterwards is C2's: the durability layer calls `prepare` and `can_retry`, and the layer itself never submits, tracks or retries.

```rust
struct Registered { records: Vec<RecordId>, exported: Vec<ExportedCoin> }   // exported: for Destination::Export only

enum Request {
    Select { from: PurseId, amount: Amount, to: Destination,     // B3 chooses the inputs
             initiator: Option<Initiator>, confirmed: bool },     // A4; an export's, none for a rebalance
    Given(PlanInput),                                              // the caller already chose: a sweep, an offload phase, a claim, a load, the registry
}

fn start(op: Op, request: Request) -> Result<Registered, Error>;
fn needs_confirmation(view: Snapshot, request: Request, now: Time, params: ChainParams) -> bool;
```

**`start`** runs these steps:

```
1. Plan       the plan callback, inside the registration's store transaction (§1), reading through it:
              for Select, B3 chooses the inputs; count the tokens (E1 tokens_needed)
              and resolve them (D1 resolve); plan the transactions (E1 plan);
              allocate a fresh index for each planned output (B2 allocate);
              for an import, keep the peer coins' secrets (B2 keep_secrets);
              return each planned transaction's call and retry policy as its policy parameters
2. Rows       the registration callback, in the same store transaction, with the records'
              identities in the order step 1 planned them: write C1's rows (C1 register);
              for an export, write the uncommitted disclosure marks (B2 mark_disclosed)
                                                                → waiting to be built
```

**Policy parameters.** Each record's policy parameters are the SCALE encoding of three values, in this order: a version number, currently 1, as one byte; the record's call, by name (G3 invariant 7); and its retry policy (G1 `Retry`). The durability layer stores them with the record and hands them back to C2 in every later release, so a release that changes the layout adds a new version and keeps decoding every earlier one. For a record whose parameters do not decode, `prepare` gives the record up and `can_retry` answers no (C2 invariants 3 and 5).

The registration is made in the domain the plan belongs to (§1): `coinage-registry` for `PlanInput::RegisterInstallation`, `coinage` for everything else. Step 1 refuses a plan with no transactions, because the durability layer would otherwise register an operation with no records. C1 `register` is called with `Admission::Import` for `PlanInput::Import`, `Admission::Reclaim` for `PlanInput::Reclaim`, and `Admission::Ordinary` for everything else. For `Select`, C1 never refuses an input B3 chose, because selection reads the state the registration commits against (invariant 1). For `Given`, the caller chose the inputs from its own snapshot; if C1 refuses one because another registration took it first, the registration rolls back and `start` fails with `InputTaken`, and the caller decides again from a fresh snapshot. Otherwise `start` fails, with nothing written, only when selection fails (`InsufficientFunds`, B3 invariant 4) or the tokens are short (`NoUnloadToken`, D1 invariant 1). On success it returns the records, and for an export the coins and their secrets (invariant 4). Nothing is built at registration. The bytes are built later, when the inputs are usable and, for an export, once the handoff is committed.

**Held-back value (A4).** For `Select`, B3 selects with `entries_first` when the initiator is `Product`, and first with `held_back` off. If that fails with `InsufficientFunds` and the request has an initiator, E3 reads the effective policy's setting for it, `held_back_for_user` or `held_back_for_products`. It selects again with `held_back` on when the setting is `Always`, or `AfterConfirmation` and `confirmed` is set; otherwise `start` fails with `InsufficientFunds`. **`needs_confirmation`** is the check G1's `export` makes before B4 `begin` (G1): it holds when the setting is `AfterConfirmation`, `confirmed` is not set, selection with `held_back` off fails with `InsufficientFunds`, and selection with `held_back` on succeeds.

**The retry policy is an argument, not a global setting**, because the right policy depends on what the caller is doing, not on the user's privacy preference. It is G1's `Retry`, given in each operation's `Op`, and stored in each record's policy parameters, where C2's `can_retry` reads it:

| Instance | Behaviour | Why |
|---|---|---|
| Until a time | Rebuild until a time the caller sets | An interactive send that the user expects to complete, or a claim due by a deadline |
| Expiry-only | Rebuild after expiry; give up after a dispatch failure or a rejection | An expired attempt says nothing about whether the request is valid; a rejected one does |
| None | Fail on the first unsuccessful verdict | A flow whose caller prefers a quick failure to a wait |

**Retrying without a limit does not mean holding inputs without a limit.** Expiry-only rebuilds for as long as attempts keep expiring, which looks like holding the inputs forever. A different mechanism ends it: a cancellation is accepted at any time and stops every rebuild after it (B4 invariant 1). So "indefinitely" means "until the caller stops it".

For the other failures, whether to retry depends on **why** an attempt failed. An attempt that expired without being included may be rebuilt while its inputs are still on chain. An attempt whose dispatch failed, or which the runtime refused, is evidence that the request itself is wrong, and rebuilding it without change spends one unload token per try (§1).

**Invariants**
1. **A selection is made on the state it commits against.** Selection runs in the plan callback, inside the registration's store transaction, and while that transaction is open no other write runs (§1). So no other registration can take a selected input before this one commits, and a selection that is rolled back holds nothing. C1 invariant 2 still checks every input in the same transaction. That check is what refuses an input that a `Given` caller chose from an older snapshot, which would otherwise be spent twice.
2. **Registration comes before building, without exception.** No bytes are built for inputs that no registration holds, and every attempt the durability layer submits belongs to a record whose rows are already durable.
3. **Once an attempt is in flight, the layer can no longer stop it.** A cancellation still stops every rebuild after it, but the attempt already in flight is settled by the durability layer's status and never by a local decision.
4. **Exported coin secrets are released as soon as the registration is durable** (step 2), before any bytes are built. So a payment can be sent without waiting for proving or finality. This is safe because a secret controls only its output account, and the registration fixes that account for every attempt (C2 invariant 5), so the payee's key works as soon as the coin lands. If the transaction never succeeds, the coin never exists, the inputs are free again (B1 invariant 2), and the index is never used again (B2 invariant 4). Every other consumer of an output waits until the output is stable.

**Verified by** driving every path against a model durability layer that calls `prepare` and `can_retry`; by interrupting at each step and checking that the store holds either the whole registration or none of it; and by checking every record status has a B4 status to project onto.

---

### Band F — Autonomy
*What the layer does when nobody asked.*

---

#### F1 · Observation

**Owns** nothing; it feeds B2. **Band:** effectful. It reads the chain, writes only B2's observation, and publishes through G2.

F1 keeps B2's observation current. It follows the chain's new blocks, reads at each block every piece of chain state that backs a record (coin storage and coin locks, ring membership and status, entry identity state, and consumed-token state), and hands the result to B2. It also reads the runtime version, so that a new runtime is checked (G3 invariant 1).

```rust
fn on_block(block: BlockRef, now: Time);   // called by the chain subscription for each new block
fn observe_now(now: Time);                 // the same reads at the latest finalized block, for a deployment that polls
```

- **`on_block`** reads, at `block`, the facts of every coin, entry and unload-token identity B2 holds, and passes them to B2 `observe(txn, facts, now)` as one `Facts` value read at that block, in one store transaction (B2). For an exported coin whose account is empty, the facts include the transaction that emptied it (B1). An item whose read failed is passed as `Read::Failed`, never as absent, so the last stored value stands (B2 invariant 6). If the runtime version at `block` differs from the version of G3's latest check, F1 then calls G3 `revalidate(block)`. Finally it publishes `Event::NewBlock(block)` through G2, on which the host's entry point is called (F2 invariant 2).
- **`observe_now`** does the same reads at the latest finalized block. It exists for a deployment that cannot subscribe (invariant 6).

**Invariants**
1. **Observation alone can never determine local state**, for two reasons. First, not all local state has a representation on chain: a coin handed to a peer out of band is in use for the coinage layer, while its account on chain is still empty. Second, an empty coin account is ambiguous: it means either "spent" or "not landed yet", and nothing F1 reads tells the two apart.
2. Therefore **an empty account must not retire the record.** An empty account is evidence, not a conclusion. Only the durability layer may draw a conclusion from it, through C2's oracle and its own rules. So F1 is not a projection of chain state onto local state.
3. F1 answers *what the coinage layer owns and in what condition*, for the balance, for selection, and for the checks made before a transaction is built. It does **not** answer *whether a transaction of this layer happened*; only the durability layer answers that. Keeping the two questions apart prevents the coinage layer from reaching a conclusion that the durability layer would contradict.
4. **It stores facts, never a verdict.** A4's thresholds are applied when readiness is read (B1 invariant 3). F1 records the facts that readiness needs, such as a ring's member count and the instant an entry joined the ring.
5. Every `Facts` value F1 passes to B2 is read at one block, never assembled from reads at two block heights.
6. The layer does not poll the chain on its own. Callers read B2's observation, and the chain subscription keeps it current through `on_block`. A deployment that cannot subscribe calls `observe_now` instead, but that is a degraded mode and must be stated as such, not treated as the design.

**Verified by** checking that no sequence of observations alone moves a record to a terminal state, and that a failed read never reaches B2 as absent.

---

#### F2 · Maintenance

**Owns** nothing; it schedules over B2's records. **Band:** pure decision with an effectful half. The effectful half reads one snapshot, starts operations through E3, and deletes imported secrets in B2; what it starts is a pure function of that snapshot, `now` and the `ChainParams` G3 holds (invariant 3).

F2 is the behaviour of G1's entry point `run_due(now) -> Due`: it runs everything that is due at `now`, starts it as operations of the layer's own, and returns at once.

The chain limits both forms that value can take, coins and entries, in different ways. A coin that has moved too often can no longer be transferred or split (§1). An entry is destroyed, together with its backing value, when its ring is cleaned up, and that cleanup runs on the clock. Two sweeps move value between the two forms:

```
        ┌──────────── coin-age recycling ────────────┐
        │                                            ▼
      Coin                                        Entry
        ▲                                            │
        └────────── ring-expiration rescue ──────────┘
```

Both thresholds come from A4. When a coin is old enough to recycle is a privacy choice. How much value may sit in the recycler at once is part of the same trade between privacy and availability.

- **`run_due(now)`** starts, in this order and from the records alone (invariant 3):
  1. a **rescue** for every entry whose ring is within the rescue margin of its expiry (invariant 1): an operation of its own, under the layer's reserved scope, that unloads one unload group into fresh coins of the purse the entries belong to (E3 `start` with `PlanInput::Spend`, `Destination::Purse`), with retry policy `Until(the ring's expiry)`;
  2. a **recycling** for every coin B1's `recycle_due` returns, and for every coin of class `Available` at the age cutoff (invariant 6): an operation of its own that recycles one coin into fresh entries of the purse it belongs to (E3 `start` with `PlanInput::Recycle`), with retry policy `Until` one token period after it starts; a recycling that gives up leaves its coin to be recycled by a later call;
  3. the next **Plan** of every open offload whose records are all stable and whose wait has ended (B4 `offload_wait`, F3);
  4. the **default reconstruction** when it is due (F4 invariant 5), and the continuation of every reconstruction a restart interrupted, from its stored progress (F4);
  5. and, in one store transaction, it deletes the secret of every peer coin whose claim transaction has succeeded finally or whose import has ended (B2 `forget_secrets`).

  An asset under an earlier installation belongs to the main purse, so a sweep's outputs for it land there (B2 invariant 5). The layer's own operations take their ids from B2's counter.
- It returns `Due`: `started` lists the operations it started, `cannot_tell` lists the purses for which it cannot tell what is due (invariant 5), and `call_again_within` is the time until the earliest instant at which something becomes due from the current records: an entry's ring reaching its rescue margin, a coin reaching the effective `recycle_at` (A4) or the age cutoff, an offload's wait ending, or the default reconstruction becoming due. `call_again_within` is zero when `cannot_tell` is not empty.

**Invariants**
1. **The rescue sweep prevents loss; the coin-age sweep keeps value spendable.** Ring expiry happens on the clock, so the rescue sweep protects value even when nobody uses the wallet. Without the rescue sweep, an idle user's entries are lost without any warning. This is the only way value disappears from a wallet whose seed and chain identity are intact. The rescue sweep unloads an entry once its ring is within the **rescue margin** of its expiry; the rescue margin is a per-network duration (A3). A coin at the age cutoff can still be recycled, so the coin-age sweep does not prevent any loss. It recycles a coin no later than the cutoff, so that the coin's value comes back as entries in the background and does not stay unspendable.
2. **Scheduling is a host obligation.** The layer is given the current time (§1), but nothing wakes it up: it can say what is due now, but it cannot arrange to be called. So it provides an entry point that runs whatever is due at a given instant and reports how long the host may wait before calling again. If a host never calls the entry point, the wallet loses value once a ring expires.

   Three mechanisms invoke the layer, and each covers a case the others do not:
   - **While the layer is running and subscribed** (F1 invariant 6), each new block counts as a call: F1 publishes `Event::NewBlock`, and the host's entry point is called on it, so a host in the foreground does not need to poll.
   - **While any transaction is open**, the durability layer keeps running in the background where the platform allows it (§1), so a registered payment or sweep completes even with the application closed.
   - **The host's periodic call** covers the remaining case: an idle wallet with no transaction in flight. That is exactly when a ring can expire without anyone noticing, so neither of the other two mechanisms can replace the periodic call.

   A platform with no background execution at all, such as a browser tab, has only the first mechanism, and a wallet there is safe only while it is open. A host on such a platform must tell the user so.
3. **Scheduling holds no persisted state.** The sweeps decide what to do from the records themselves (a coin's age, an entry's ring deadline, an offload's stored wait), not from the time elapsed since the last call. So the entry point is safe to call at any frequency, and a restart loses nothing. What one call starts depends only on those facts: rescues ordered by ring deadline, then recycling ordered by coin age, with ties broken by purse, installation and index. Each rescue is a separate operation for one unload transaction, and each recycling is a separate operation for one coin, so a short supply of tokens delays only the rescues that the supply cannot cover (D1 invariant 1). A sweep that consumes an asset from an earlier installation creates its outputs in the main purse (B2 invariant 5). One call also runs Plan for every open offload whose records are all stable and whose wait has ended (F3 invariant 4), starts the default reconstruction when it is due (F4 invariant 5), and resumes every reconstruction that a restart interrupted. The layer's own operations take their ids from a counter in B2.
4. A call never waits. Anything that needs a second look, such as an entry whose ring has not yet filled, is reported and handled at the next call.
5. **An empty result is not evidence that observation ran**, and the sweep reports this explicitly instead of leaving it to the caller. An entry that nobody has observed has no recorded ring deadline, and a missing deadline looks the same as a ring that still accepts members. So the sweep reports **three** outcomes: work done (`started` is not empty), nothing due (both lists are empty), and *cannot tell* (`cannot_tell` is not empty). *Cannot tell* names the purses whose observation is missing or stale, or whose due rescue cannot get a token or cannot start because the runtime fails a check (G3 invariant 1). One call can report work done and *cannot tell* together. If *cannot tell* were reported as *nothing due*, a wallet that never read the chain would report a clean sweep while its rings expire.
6. **The recycling sweep enforces A4's `max_unavailable_share`** through B1's `recycle_due`, and it is the only component that does. The unavailable value counts every coin and entry still minting and every entry not yet selectable, so the outputs of the sweep's own recyclings still in progress use up headroom. A sweep that did not count them would use the same headroom again in every batch. A coin is admitted while any headroom is left, even if it overshoots the cap: a coin worth more than the whole headroom is recycled instead of waiting for the age cutoff, and the next coin finds no headroom. While D1's `running_low` holds, no coin is admitted (D1 invariant 2). The rescue sweep is **exempt** from the cap: it moves value from entries back to coins, and an entry whose ring is expiring is not a privacy choice the user can decline. If the cap applied to the rescue sweep too, a privacy setting could stop the sweep that prevents loss. Recycling a coin at the age cutoff is exempt from the cap and from the reserve: the coin cannot be spent, and holding it back would only keep it unspendable longer, which A4's first outer limit forbids.

**Verified by** advancing a model clock past each deadline and checking that the sweep fires; by the negative case of invariant 5: when the deadline is *absent* rather than distant, advancing the clock by years changes nothing; and by checking that `call_again_within` never lies past the next instant at which a deadline is reached.

---

#### F3 · Offload phases

**Owns** nothing; it decides each phase of an `offload` operation. **Band:** pure decision with an effectful half. The effectful half reads a snapshot and then starts a phase through E3 or stores a wait or an end in B4; the choice of phase is a pure function of the snapshot, the remaining amount and `now`.

F3 is the behaviour of G1's `offload(op, from, amount, to, initiator, confirmed)`: it moves `amount` out of the purse `from` into the external asset's account `to`. Offload is the only operation that is planned in more than one phase. Each phase registers its transactions under the offload's own operation key through E3 `start`, and B4 stores the offload's wait and its end.

```
        ┌───────────────────────────────────────────────┐
        ▼                                               │
      Plan ──entries cover the amount──▶ Offboard ──▶ Done
        │                                               ▲
        ├──coins cover the deficit──▶ Recycle ──────────┘
        ├──entries still ripening──▶ Wait(ready_at) ─────┐
        ├──only transient holders cover it──▶ Wait(short)┤
        └──nothing covers it──▶ Failed                   │
        ◀───────────────────────────────────────────────┘
```

**Plan** runs at the offload's start and then from the entry point (F2), and decides the next phase from a fresh snapshot of `from` and the remaining amount (the requested amount less the value already offboarded, B4 invariant 6):
- **Offboard** when usable entries cover the remaining amount: it selects them with B3 `select` (`Forms::EntriesOnly`, the remaining amount, `held_back` as below), and starts their unload into `to` (`PlanInput::Spend` with `Destination::External(to)`).
- **Recycle** when they do not, and usable coins cover the deficit, which is above zero: it takes usable coins of `from` in B3's order, up to the deficit, and starts their recycling into fresh entries of `from` (`PlanInput::Recycle`). This recycling is the caller's request, so A4's `max_unavailable_share` does not apply to it (F2 invariant 6).
- **Wait(ready_at)** when what is missing is in entries still **ripening**: entries counted as *held back* whose wait ends at a known time (their dwell), with `ready_at` the earliest such time. B4 stores `ready_at` (B4 `record_offload_wait`), the offload's status is `Waiting`, and the next Plan runs at the first `run_due` at or after `ready_at`.
- **Wait(short)** when what is missing is held only by **transient holders**: value counted as *arriving* (still being minted, inside a chain lock, or an input of another operation). B4 stores a wait with no time, the status is `Waiting`, and the next Plan runs at the next `run_due`.
- **Failed** when nothing in `from`, usable now or later, covers the remaining amount.

**Held-back value (A4).** Plan reads the effective policy's setting for the offload's initiator. When it is `Always`, or `AfterConfirmation` and the offload was started with `confirmed`, held-back entries are usable now (B3 `held_back`): an entry a recycle phase created is offboarded once its key is included, without waiting for the readiness rules. Otherwise Plan waits for held-back entries as below. **`needs_confirmation(view, from, amount, initiator, confirmed, now, params) -> bool`** is the check G1's `offload` makes before B4 `begin`: it holds when the setting is `AfterConfirmation`, `confirmed` is not set, and the entries of `from` that are selectable now, with this period's free tokens, do not cover `amount`. It is pure, like the choice of phase.

The **deficit** is the remaining amount minus the value of the entries of `from` that are usable now, still ripening, or *arriving*. Recycle takes coins only to cover the deficit. When the deficit is zero but usable entries do not cover the remaining amount, Plan chooses one of the two waits.

**Invariants**
1. **Plan counts arriving entries toward the amount.** An entry that a recycle phase just created is *arriving* until F1 has observed it in a ring, and an entry without a known ring cannot be offboarded. If Plan did not count it, every pass would recycle more coins for the same deficit.
2. **Offboarding takes recycled entries only at a later Plan**, once their recycling is final and they are usable. So an offload has no dependencies between its own transactions.
3. **Surplus is always reloaded into fresh entries within the same transaction.** If the surplus landed as a free coin, it would link the entries' anonymity set to a fresh account, and preventing that link is the purpose of the recycler.
4. **The phase is read from the offload's records**, so after a restart the offload resumes from where its records stand, and the entry point runs Plan for it (F2 invariant 3). **Its end is stored** in B4: Plan's `Done` or `Failed`, or a requested cancellation once the offload's records are stable. Plan never runs after the stored end. A phase with a transaction that failed after reaching the chain also ends the offload, so a failure that repeats does not cost one token per Plan.
5. **An offboard never leaves its destination below the external asset's minimum balance** (A3). The chain would refuse such an offboard after the unload token was already spent. `offload` refuses an amount that could not lift an empty destination to the minimum balance (`BelowMinimumBalance`). While the destination is below the minimum balance, each offboard carries at least the shortfall. When the usable entries of no single ring reach the shortfall, Plan ends the offload `Failed`.

**Verified by** driving the state machine over generated purse states and checking that every cycle terminates and that no path offboards an entry whose ring is unknown.

---

#### F4 · Wallet reconstruction

**Owns** nothing; it repopulates B2. **Band:** effectful. It reads the chain and writes only B2. F4 also defines the installation registry's format, which G3's registration writes.

F4 rebuilds a wallet from the seed when the durable local state is lost. This is different from what C2 does: C2 decides the outcome of transactions that are still open, while F4 finds assets that the local store no longer knows about. F4 is the behaviour of G1's `reconstruct(op, purses, deep)`, and of the default reconstruction the entry point starts (invariant 5).

```rust
struct ScanConfig {          // part of the layer's configuration, given at start (G1 start)
    batch: u32,              // indices derived and read per request
    gap: u32,                // consecutive empty batches that end a default scan of one index space
    deep_gap: u32,           // the same, for a deep scan; larger than gap
    purse_gap: u32,          // consecutive empty purses that end Purses::Walk
}

// the installation registry
type SealedInstallation = [u8; 60];                               // nonce(12) ‖ ciphertext(32) ‖ tag(16)
fn seal_installation(id: InstallationId) -> SealedInstallation;   // the payload of E1's Call::AppendRegistryRecord
fn open_installation(record: Vec<u8>) -> Option<InstallationId>;  // none: not sealed under this seed's key, or malformed
fn read_registry(at: BlockRef) -> Option<Vec<Vec<u8>>>;           // all records; none: the read failed
```

- **`reconstruct(op, purses, deep)`** (G1) is an operation with no transaction of its own (B4): it is `Preparing` while it scans and `Done` when every namespace it covers is scanned. It first reads the registry (`read_registry`), opens every record (`open_installation`), and adds every installation identifier B2 does not know (B2 `add_installations`). Then it scans the namespace `(purse, installation)` of every purse in `purses` (the main purse always included; for `Purses::Walk`, product purses from zero upwards until `purse_gap` consecutive purses are empty) under every known installation, the current one included. Scanning continues from the progress B2 stores for each installation (B2 `installations`), and records progress after every batch (B2 `record_scan`), so a restart resumes where it stopped. F4 defines `ScanProgress.resume` as the purse, the index space and the next index being scanned under that installation, and sets `first_scan_done` once every namespace of the installation has been scanned once.
- **Scanning one namespace** treats its two index spaces, coins and entries, separately, because A2 derives them under separate trees, so an index in one space cannot name an object in the other. For each space it derives `batch` consecutive accounts or member keys, reads them in one request at a finalized block, and adopts every index for which the chain returns something and which B2 does not already hold (B2 `adopt`), with the exponent the chain shows (invariant 3). Adopting never advances a counter, because what F4 finds was allocated by an earlier instance. The space is done after `gap` consecutive empty batches, or `deep_gap` when `deep` is set.
- **`seal_installation`** seals an installation identifier as invariant 4 describes. It is deterministic, so sealing one identifier twice gives the same bytes. **`open_installation`** reverses it under this seed's key. **`read_registry`** returns the records `getCoinageInstallations` returns for the `//datastore` address.

**Invariants**
1. **Reads are bulk; derivation is not.** One request reads a whole batch, but every index needs its own local key derivation, and the entry side uses the slower scheme. So concluding that a namespace is *empty* costs a full run of batches before any useful answer.
2. **It reconstructs assets, not operations.** A scan cannot rebuild a record, its rows or its dependencies: they live in the store and are lost with it. A transaction submitted before the wipe may still land, and nothing local expects its outputs. Those outputs show up later as assets that a scan finds, and a reconstruction that runs before they land misses them until the next reconstruction.
3. **Local state that the chain cannot witness is lost, and the value it resets to matters.** One timer exists only locally: the instant an entry's key was first observed included in its ring's root, from which the readiness rules' dwell is counted. It cannot be recovered. Inclusion is a chain event, but it can only be observed when it happens, and a reconstruction comes later. A missing inclusion instant must therefore be read as *now*, not as *long ago*. Treating the dwell as satisfied just because nothing records the inclusion would bring back exactly the timing correlation that the dwell is meant to prevent. So a reconstructed entry waits out its dwell again from the moment it is found, and becomes eligible later than it would have, never sooner.
4. **The installation registry is the only thing that survives a wipe, and it is stored on chain.** Each installation is registered when it is minted (G3 invariant 4), and F4 reads the registry back. Reconstruction adds every installation identifier it does not already know, and scans each of them, including the current one. This is why the seed alone is enough: installation identifiers are random by design, so no derivation can recover them, and a wallet that could not read them back would find only the namespace it had just minted.

   The registry is the shipped wallet's, and its format is fixed by compatibility (§5): a seed recovered on one platform must find the installations that another platform registered.
   - **Where.** The *AccountDataStore* contract on Asset Hub [AccountDataStore]: `registerCoinageInstallation(bytes)` appends a record, `getCoinageInstallations(address)` returns them all.
   - **Who.** A dedicated `//datastore` sr25519 account derived from the seed, mapped for the contract runtime. It pays for the call and the storage deposit in PGAS, which it claims with a ring proof of the user's personhood. As with the unload tokens, nothing needs funding from outside (D1).
   - **What.** Each identifier is sealed with ChaCha20-Poly1305 [RFC 8439] as `nonce(12) ‖ ciphertext(32) ‖ tag(16)`, with no associated data. The key is `K = BLAKE2b-256("encryption", key = S)`, where `S` is the `//datastore` sr25519 secret in its 64-byte ed25519-compatible form: the secret scalar multiplied by 8, as 32 little-endian bytes, followed by the 32-byte nonce half of the secret. This is the form polkadot-js and the Android SDK hold; the iOS app converts its canonical secret to it. The nonce is the first 12 bytes of `BLAKE2b-256("nonce" ‖ id, key = BLAKE2b-256("nonce-key", key = K))`, so registering one installation twice writes the same bytes. In each `BLAKE2b-256(m, key = k)`, `m` is the message, hashed as its ASCII bytes where it is a label, and `k` is BLAKE2b's key parameter. To open a record, F4 reads the nonce from the record itself, so a record written with a random nonce by another platform opens too. A record that does not open under this seed's key is skipped.

   Anyone who knows the `//datastore` address can count the records, and learn nothing more: the records hold no purse, no index and no amount, and the identifiers are encrypted. The registry also does not reveal the installation node's public key, which A2 requires to stay private: an installation *identifier* is a chain-code segment, not the installation node's public key. Deriving anything under an installation still needs the seed.
5. **An installation that is registered but never scanned looks the same as one that holds nothing**, so scan progress is tracked per installation, and an installation is not considered done until its first scan completes. A failed read of the registry only delays discovery: installations learned on an earlier run are still scanned. The default reconstruction covers the main purse and every purse a restored metadata backup lists. It is due whenever the registry lists an installation not yet scanned (the shipped wallet does this at each launch), and the entry point starts it (F2 invariant 3). So a wipe is followed by recovery without any caller, before the rings of forgotten entries expire without being rescued. The default reconstruction is due no earlier than one longest validity window, finalized, after the current installation was minted, so that every transaction from before the wipe has landed or died first.

**What the registry does not carry, and the consequences.** Three kinds of information are lost with the store. Each leaves a residual risk that the design accepts and does not remove:

**The high-water marks are lost, so the rule for stopping a scan works worse the more the wallet has been used.** A high-water mark records how far an index space was allocated. A consumed index leaves nothing on chain: a spent coin's account is empty, and an unloaded entry's key is removed from its ring. Both index spaces therefore become sparse from the bottom as the wallet is used, and the sparse part only grows. A scan that stops after a run of empty batches therefore stops at the first long run of consumed indices, and the holder cannot tell a short result from a complete one. The mitigation is to make the scan's bound explicit: the default scan stops after `gap` empty batches, and the holder can request a deeper scan (`deep`, stopping after `deep_gap`) that goes further and takes longer. Publishing the high-water marks instead would reveal the wallet's transaction count without making anything safer.

**The disclosure marks are lost, so a rescan cannot tell an owned coin from a coin whose secret was given away.** An exported coin's account still holds its coin. That is what makes export cheap: a coin that already has the needed denomination leaves under its own secret, with no transaction. The only record that control was shared is the disclosure mark of C1 invariant 4, and that mark is local. So a reconstruction adopts every coin the wallet ever exported, which overstates the balance and can eventually select value that a third party can spend. The alternatives are worse: publishing the marks would reveal exactly which coins were handed over and to how many parties, and moving a coin out of the namespace before disclosing its secret costs one transaction per export.

**Purse identifiers are lost, and finding them again is possible but expensive.** Nothing on chain lists purse identifiers, and publishing them would link a seed to the products that asked for them. But unlike an installation identifier, a purse identifier is not random: product purses count up from zero (A2). So a reconstruction can walk purse identifiers the way it walks indices, and stop after `purse_gap` empty ones. The cost is why this remains a residual risk: each candidate purse needs a full scan of both index spaces before it can be called empty (invariant 1), so the work multiplies by the number of purses probed. The same gap problem also applies at the purse level: if the only surviving purse lies well past a run of closed purses, the reconstruction does not find it.

So the default reconstruction recovers the main purse and every purse the backup lists, and other product purse identifiers are walked only on request (`Purses::Walk`). Whatever either kind of reconstruction finds under an earlier installation counts toward the main purse (B2 invariant 5).

Allocation does not need the reconstruction to be complete. A purse holds only the current installation's coins, so an identifier reused after a reinstall cannot show earlier value in that purse, wherever the scan stopped.

**Verified by** seeding a model chain from a known seed, discarding local state, reconstructing from seed and registry alone, and comparing against the original records, including the adversarial cases: a wallet with several registered installations, one whose live records sit beyond the gap bound, and one containing a previously exported coin; and by fixed vectors for `seal_installation` and `open_installation`, including a record sealed with a random nonce.

---
### Band G — Surface
*What the layer above sees: the primitives (G1), the streams (G2) and the start of the layer (G3). Nothing here is durable; G1, G2 and G3 hold the layer's only in-memory state.*

---

#### G1 · The primitive API

**Owns**, in memory, the `Funding` capability of each top-up that has not ended (invariant 6), kept in the `Running` handle, where C2's `prepare` reads it (C2). **Band:** thin surface.

G1 consists of the primitives and the error and event vocabularies. The primitives are: the purse lifecycle (create, query, list, resolve, rebalance, delete), top-up, export, reclaim and import, offload to an external account, the maintenance sweep, and reconstruction.

A payment to someone else is an export on the payer's side and an import on the payee's side. The caller provides the transport between them, for example an RFC-17 cheque or a chat message. Top-up moves value from the external asset into coinage, and offload moves it out; all other primitives stay inside the layer.

**Interface.** The code below is everything layer 1 offers, and it is the one place where the public types are defined. Each line names the component whose rules it follows. `start` returns a `Layer`, and every other function is an operation on that `Layer`. G2 defines how the streams behave, and G3 how the layer starts. The notation is defined in section 2; it fixes the meaning, not a wire format.

```rust
// ---- Vocabulary ----
type PurseId    = u32;        // A2; the main purse is u32::MAX
type Amount     = u64;        // cents (A1); narrowed with a checked conversion at the RFC boundary
type Exponent   = u8;         // a coin is worth 2^e cents (A1)
type Time       = u64;        // from the clock (§1)
type Duration   = u64;
type AccountId  = [u8; 32];   // an on-chain account: a coin's, a funding source, a destination
type CoinSecret = [u8; 64];   // the sr25519 secret controlling one coin account: scalar ‖ nonce in the canonical layout only (invariant 5)
type Signature  = [u8; 64];
type TxHash     = [u8; 32];
type BlockRef   = (u32, [u8; 32]);   // a finalized block: number and hash

struct OpKey { scope: Vec<u8>, id: [u8; 32] }       // B4: the caller's identity for an operation; the empty scope is the layer's own
enum Retry { Until(Time), ExpiryOnly, Never }      // E3's three retry policies
struct Op { key: OpKey, retry: Retry }

enum Started {                 // what starting a long-running operation returns
    New,
    Joined,                    // the key names a live operation with the same arguments (B4)
    Finished(Completion),      // the key names one that has completed (B4 invariant 5)
}

trait StoreTxn {}              // an open transaction on the shared store (store-contract.md)

// ---- Starting ----
struct Interfaces {            // §1's interfaces, supplied by the host
    chain: Chain,              // state reads at a named block, and the runtime version
    durability: Durability,    // the durability layer (§1)
    store: Store,              // the shared store (store-contract.md)
    clock: Clock,              // the current time
    seed: Seed,                // the root entropy, or a handle to it
    randomness: Randomness,    // the installation identifier draw
    policy: PrivacyPolicy,     // A4: the initial policy, stored only on a fresh store
}
trait Chain {} trait Durability {} trait Store {} trait Clock {} trait Seed {} trait Randomness {}   // §1 says what each supplies
trait Layer {}                 // the running layer; every function below is called on it

fn start(env: Interfaces, config: ChainConfig, scan: ScanConfig,
         purse_backup: Option<Vec<u8>>, running: Running) -> Result<Layer, StartError>;   // G3; ChainConfig: A3; ScanConfig: F4; Running: C2
enum StartError {              // G3 invariant 1
    UnsupportedRuntime { check: String },   // A3 validate: the check that failed
    UndecodableStore,
}

// ---- Purses ----
fn create_purse(name: String, tag: Vec<u8>) -> Result<PurseId, Error>;     // B2
fn query_purse(purse: PurseId) -> Result<PurseInfo, Error>;
fn list_purses() -> Vec<PurseInfo>;
fn resolve_purse(purse: PurseId) -> Result<PurseId, Error>;                // B2: where a deleted purse's value went
fn rebalance(op: Op, from: PurseId, to: PurseId, amount: Amount) -> Result<Started, Error>;
fn delete_purse(op: Op, purse: PurseId, drain_into: PurseId) -> Result<Started, Error>;  // invariant 4

struct PurseInfo { id: PurseId, name: String, created: Time, tag: Vec<u8>, balance: Balance }
struct Balance { spendable: Amount, held_back: Amount, arriving: Amount }   // B2's partition

// ---- Value in and out of coinage ----
fn top_up(op: Op, into: PurseId, amount: Amount,
          funding: Funding) -> Result<Started, Error>;                 // invariant 6
fn offload(op: Op, from: PurseId, amount: Amount, to: AccountId,
           initiator: Initiator, confirmed: bool) -> Result<Started, Error>;  // F3; Initiator: A4

trait Funding { fn account() -> AccountId; fn sign(payload: Vec<u8>) -> Option<Signature>; }  // None: the signer declines

// ---- Payments: export on the payer's side, import on the payee's ----
fn export(op: Op, from: PurseId, amount: Amount,
          initiator: Initiator, confirmed: bool) -> Result<Export, Error>;  // coins at once (E3 invariant 4)
fn commit_export(export: OpKey, within: Option<StoreTxn>) -> Result<(), Error>;  // invariant 3; None: a store transaction of its own
fn release_export(export: OpKey) -> Result<(), Error>;                     // invariant 3: never committed
fn reclaim(op: Op, export: OpKey, into: PurseId) -> Result<Started, Error>;  // C1 invariant 4: `into` must be open
fn import(op: Op, into: PurseId, coins: Vec<CoinSecret>) -> Result<Started, Error>;
fn importing(coins: Vec<CoinSecret>) -> bool;  // a claim without a stable verdict holds one of them (C1)

struct Export { started: Started, coins: Vec<ExportedCoin> }   // the same coins however often it is joined
struct ExportedCoin { account: AccountId, secret: CoinSecret, exponent: Exponent }
enum ExportStatus { NotYetMinted, AwaitingPayee, Claimed { stable: bool, by: TxHash }, Reclaimed { stable: bool }, NeverMinted } // B1

// ---- Maintenance and recovery ----
fn run_due(now: Time) -> Due;          // F2; called by layer 2's run_due, which is the host's one entry point
struct Due {                           // F2 invariant 5: both lists empty means nothing was due
    started: Vec<OpKey>,
    cannot_tell: Vec<PurseId>,
    call_again_within: Duration,
}

fn reconstruct(op: Op, purses: Purses, deep: bool) -> Result<Started, Error>;  // F4
enum Purses { MainOnly, These(Vec<PurseId>), Walk }     // the main purse is always included

// ---- Policy, backup, parameters ----
fn set_policy(policy: PrivacyPolicy) -> Result<(), PolicyError>;   // A4 check; re-classifies, migrates nothing
fn purse_backup() -> Vec<u8>;                  // §1: the one part of the store that may be backed up
fn asset_unit() -> u128;                       // A3: the external asset's amount in one cent, the instance's unit
fn recoverable() -> Recoverability;            // G3 invariant 4
enum Recoverability { Registered, Pending { since: Time } }

// ---- Following an operation (G2) ----
fn status(op: OpKey) -> Result<Stream<StatusItem>, Error>;   // current item first, then changes
fn completion(op: OpKey) -> Option<Completion>;               // B4 invariant 5
fn cancel(op: OpKey) -> Result<(), Error>;                    // B4 invariant 1
fn balance(purse: PurseId) -> Result<Stream<Balance>, Error>;  // G2 invariant 1
fn export_status(export: OpKey) -> Result<Stream<Vec<(AccountId, Exponent, ExportStatus)>>, Error>;
fn events() -> Stream<Event>;                                  // changes only, never replayed

struct StatusItem { status: Status, progress: Progress }
enum Status { Preparing, Waiting, Submitted, Settling, Done, PartiallyDone, Failed(Failure) } // B4
struct Progress { settled: Amount, in_flight: Amount }   // B4 invariant 6; PartiallyDone's amount is `settled`

struct Completion {                                       // B4
    key: OpKey,
    status: Status,                                       // terminal
    progress: Progress,
    transactions: Vec<Settled>,
    coins: Vec<(AccountId, CoinResult)>,                  // import and reclaim: one per coin
}
struct Settled { hash: TxHash, block: BlockRef, consumed: Vec<AccountId>, created: Vec<AccountId> }
                                                          // coins only, never a recycler entry
enum CoinResult {
    Moved,         // import: claimed; reclaim: taken back
    TakenByOther,  // import: sniped; reclaim: the payee claimed first
    Absent,        // never seen on chain while the retry policy allowed: a bad secret, or never minted
    NotClaimed,    // present but unmoved when the operation ended: locked, at the chain's maximum coin age, or not this layer's coin (invariant 5)
}

// ---- Failures ----
enum Error {                          // failing to start, or a query that cannot be answered
    PurseNotFound(PurseId),           // unknown, closed, or (as a destination) with a deletion open
    MainPurse,                        // the main purse cannot be deleted
    UnsupportedRuntime { check: String },   // A3 validate fails for the current runtime (G3 invariant 1)
    NeedsConfirmation,                // A4: the spend needs held-back value the user has not confirmed
    PurseHoldsUnmovableValue,         // invariant 4
    DeletionOpen(PurseId),            // invariant 4: the purse already has a deletion open
    InsufficientFunds { available: Amount },        // B3 invariant 4: short even counting what is waiting
    NoUnloadToken,                    // D1 invariant 1
    InputTaken,                       // E3: another registration took an input the request named first
    BelowMinimumBalance,              // F3 invariant 5
    InvalidSecret(u32),               // invariant 5: the coin secret at this position is not a canonical key
    UnknownOperation(OpKey),
    KeyReused(OpKey),                 // the key names an operation with different arguments
    ReservedScope,                    // the caller used the empty scope, which is the layer's own
    NotCancellable,                   // B4 invariant 1
    NotAnExport(OpKey),               // commit, release or reclaim on an operation that is not an export
    ExportClosed(OpKey),              // commit of an export already released or ended, or release after the commit
    ExportNotCommitted(OpKey),        // reclaim of an export not yet committed: release it instead
}
enum Failure {                        // why a started operation ended Failed
    Cancelled,                        // B4 invariant 1
    NotCommitted,                     // an export released, or never committed before a restart (invariant 3)
    Refused,                          // the chain refused, and the retry policy allows no more (§1)
    Expired,                          // attempts kept expiring until the retry policy gave up
    NothingMoved,                     // import or reclaim moved no coin; see Completion::coins
    GivenUp,                          // a record was given up (C2 invariant 5)
}

// ---- Events (G2) ----
enum Event {
    Resynced,                                     // G2 invariant 5: the first item of every events() stream
    PurseCreated(PurseId),
    PurseDeleted(PurseId),
    PurseMetadataChanged,                         // refresh the backup (§1)
    Status(OpKey, StatusItem),
    ExportStatus(OpKey, AccountId, ExportStatus),
    Recoverability(Recoverability),               // G3 invariant 4
    NewBlock(BlockRef),                           // F1 observed a new block; the host's entry point may run on it (F2 invariant 2)
}
```

**Behaviour of each primitive.** Every long-running primitive (one that takes an `Op`) first checks its arguments, then asks B4 to start the operation under `op.key` or to join the one it already names (B4). Only a new operation goes on to the step described below; a joined one returns `Joined`, and a completed one returns `Finished`. A caller's `op.key` with the empty scope is refused (`ReservedScope`): that scope is reserved for the operations the layer starts itself, the sweeps, offload phases, reconstructions and the installation registration. While the current runtime fails A3's check, every primitive that starts an operation fails with `UnsupportedRuntime` (G3 invariant 1).

- **`start`** constructs the layer, as G3 describes, and returns only after G3 invariant 5's start sequence has finished. So no other primitive can be called before the layer accepts operations.
- **`create_purse`** allocates a purse in B2 and returns its identifier. It emits `PurseCreated` and `PurseMetadataChanged`.
- **`query_purse`** and **`list_purses`** read B2's stored state and balance; they read no chain. A closed purse is not listed, and querying it fails with `PurseNotFound`. A purse with a deletion open is still listed until it closes.
- **`resolve_purse`** returns the purse a purse's value goes to: the purse itself while it is open, otherwise the purse its deletion drains into, followed through repeated deletions until an open purse (B2). It fails with `PurseNotFound` only for an identifier that never named a purse.
- **`rebalance`** moves `amount` from `from` to `to`. Both must be open, and neither may have a deletion open (`PurseNotFound(to)`, `DeletionOpen(from)`). It starts the operation through E3 with `Request::Select { from, amount, to: Destination::Purse(to), initiator: None, confirmed: false }`. Whether the value moves by a direct transfer or through the recycler follows A4's `move_through_recycler` (E1).
- **`delete_purse`** refuses the main purse (`MainPurse`), a purse that is unknown or closed (`PurseNotFound(purse)`), a purse that already has a deletion open (`DeletionOpen(purse)`), a `drain_into` that equals `purse`, is not open or has a deletion open (`PurseNotFound(drain_into)`), and a purse holding value that cannot move (`PurseHoldsUnmovableValue`). Otherwise it records in B2 that the purse has a deletion open, with `drain_into` as its target (B2 `begin_delete`), and starts one operation through E3 that selects everything the purse holds into `drain_into` (invariant 4). The operation ends once B2 records the purse closed (B2 `close_purse`), and the layer then emits `PurseDeleted` and `PurseMetadataChanged`.
- **`top_up`** loads `amount` cents into recycler entries of `into`, paid from the funding account in the external asset at the instance's `asset_unit` (A3). It starts the operation through E3 with `Request::Given(PlanInput::Load { from: funding.account(), amount, into })`; E1 plans one load per transaction (§6). While the funding account cannot cover a load, `prepare` waits (C2 invariant 5), until the retry policy's deadline. An account covers a load when it holds, at the finalized head, the load's value in the external asset and the asset's minimum balance besides, because a load keeps the paying account alive. The loads of one funding account are built one at a time, in registration order (E2 invariant 4). `Funding::sign` is called once per attempt, when E2 assembles it. If the signer declines, the record is given up and the operation ends `Failed(GivenUp)`; after a restart the capability is gone and the same happens (invariant 6).
- **`offload`** moves `amount` out of `from` to the external account `to`, through the recycler, in the phases F3 describes. It refuses an amount that could not lift an empty destination to the external asset's minimum balance (`BelowMinimumBalance`, F3 invariant 5). Before B4 `begin`, it fails with `NeedsConfirmation` when F3's `needs_confirmation` holds. F3 starts each phase's transactions under the operation's key, and B4 stores the offload's wait and end.
- **`export`** fails with `NeedsConfirmation` before B4 `begin` when E3's `needs_confirmation` holds. Otherwise it starts the operation through E3 with `Request::Select { from, amount, to: Destination::Export(from), initiator: Some(initiator), confirmed }` and returns the coins and their secrets at once, with uncommitted disclosure marks (invariant 3, E3 invariant 4). Joining an export returns the same coins.
- **`commit_export`** commits the export's disclosure marks (B2) inside `within`, or in a store transaction of its own when `within` is none (invariant 3). Committing again returns `Ok`. It fails with `ExportClosed` for an export that was released or has ended, and with `NotAnExport` or `UnknownOperation` for any other key.
- **`release_export`** gives up an export that was not committed: its uncommitted marks are deleted (B2 `release_disclosed`, B4 `record_release`), its minting records are given up at their next build (C2 invariant 5), and the export ends `Failed(NotCommitted)`. Releasing it again returns `Ok`. After the commit it fails with `ExportClosed`; a committed export is ended by `reclaim`.
- **`reclaim`** takes back the coins of a committed export into fresh accounts of `into`, which must be open. It fails with `ExportNotCommitted` before the commit. It starts the operation through E3 with `Request::Given(PlanInput::Reclaim { coins, into })`, registered with `Admission::Reclaim` (C1 invariant 4). A coin not yet minted is reclaimed by giving up its minting (C2 invariant 5). The result is judged per coin (B4): `Moved` when taken back or never minted, `TakenByOther` when the payee claimed first.
- **`import`** claims each coin into a fresh account of `into`, which must be open. It refuses the whole call with `InvalidSecret(i)` if the secret at position `i` is not a canonical key (invariant 5). It starts the operation through E3 with `Request::Given(PlanInput::Import { coins, into })`, registered with `Admission::Import`. A coin not yet on chain is waited for while the retry policy allows. A coin of another instance or with a negative exponent is left where it is and reported `NotClaimed` (invariant 5). The result is judged per coin (B4).
- **`run_due`** runs whatever maintenance is due at `now` (F2).
- **`reconstruct`** starts a reconstruction of the purses named (F4).
- **`set_policy`** refuses a policy that A4's `check` refuses (`PolicyError`). Otherwise it stores the policy in B2 exactly as given (B2 `set_policy`), and every reader applies A4's `effective` to it, so the outer limits hold without changing what is stored. Classifications change with it; no record changes (A4).
- **`purse_backup`** returns B2's purse backup: `SCALE(Vec<PurseRecord>)` of every purse ever created, closed ones included (B2 `purse_backup`). The caller stores these bytes and hands them back to `start`; it never interprets them. `PurseMetadataChanged` says when to take a new backup.
- **`asset_unit`** returns the instance's unit from the `ChainParams` G3 holds.
- **`recoverable`** reports whether the current installation's registration has succeeded (G3 invariant 4).
- **`status`**, **`balance`**, **`export_status`** and **`events`** open the streams G2 describes. `status` and `export_status` fail with `UnknownOperation`, and `balance` with `PurseNotFound`.
- **`completion`** returns an operation's completion once it has ended, and none before (B4).
- **`cancel`** requests the cancellation of an operation (B4 invariant 1). It fails with `NotCancellable` for an export and for the layer's own operations, and with `UnknownOperation` for an unknown key. Cancelling an operation that has already ended returns `Ok` and changes nothing.

Three rules are not visible in the signatures:
- Sweeps and reconstructions that the layer starts itself run under the empty scope, which is reserved for the layer, so their keys never collide with a caller's keys.
- An export that is released, or left uncommitted at a restart, ends `Failed(NotCommitted)`, and its value is back in the purse. Nothing it would have minted was built, because minting waits for the commit (C2 invariant 5). A commit after that fails with `ExportClosed`, so the caller's store transaction carrying the secrets rolls back.
- `Absent` cannot distinguish a bad secret from a coin whose minting never landed. The payee alone cannot see the difference; the payer's export status shows it.

**Invariants**
1. Every long-running primitive returns its handle **before** any attempt of its transactions is submitted. A failure to *start* is returned as an error from the call; a failure of a *started* operation is a terminal item on its status stream. This separation is what makes cancellation possible to express (B4 invariant 1).
2. The surface adds **no status vocabulary of its own**. It reports B4's statuses. Mapping the coinage layer's progress and the durability layer's verdicts onto B4's statuses is a translation, not a second source of truth.
3. **Export has two phases, and the second phase is atomic with the caller's own write.** `export` registers the transactions, reserves their inputs, writes uncommitted disclosure marks and returns the secrets at once (E3 invariant 4). `commit_export` commits the marks **inside the caller's store transaction**, the one in which the caller saves the message or cheque that carries the secrets. Either both become durable or neither does. So there is no moment at which the secrets have left while the coins are still spendable here, and no moment with the reverse. The minting is built only after the commit (C2 invariant 5). Until the commit, the export can be released (`release_export`), and a restart releases it.

   A caller whose records are not in the shared store cannot pass its store transaction. It calls `commit_export` with no transaction, and must follow an ordering rule instead: commit **no later than** the moment it makes the secrets durable anywhere else.
   - Commit first, crash before sending: the coins are marked but nobody has their secrets, and reclaim recovers them.
   - Send first, crash before committing: a restart releases coins that the payee may still receive, and then both sides spend them.

   Only the second order loses money.
4. A purse being deleted closes **only after the chain confirms** that its value has left, and a purse still holding value that cannot move is refused, not closed with the value still in it. Closing discards records, and if a record is discarded while its account still holds a coin, nobody finds that value again without a full rescan. So the drain is one selection of everything the purse holds, with coins still being minted into the purse taken as dependencies (B3). From the call on, B2 records the purse as having a deletion open, with its drain target (B2): no new operation may name the purse as its destination (`PurseNotFound`), and a second deletion is refused (`DeletionOpen`). The purse closes only once nothing in it is arriving and no operation that names it as destination is still open, and B2 then records it closed.
5. The export/import pair is the **only** place where coin secrets cross the API, and no primitive returns a key above a coin or an entry in the derivation tree, including the installation node's public key (A2). Export treats a coin that already has the needed denomination as needing no transaction: control passes with the secret. Import keeps a supplied secret only until its claim is final, because every rebuild of the claim transaction signs with it (C2 invariant 5). An import leaves a coin with a negative exponent (A1), or a coin of another instance (§1), where it is.

   **Every 64-byte sr25519 coin secret is in the canonical layout, and only that layout is read**: a coin secret in an export, an import or a cheque. The canonical layout is the 32-byte encoding of the secret scalar `k` (little-endian, `k < ℓ`, as in A2), followed by the 32-byte nonce. Both shipped apps write and read this layout. A second layout is in use for sr25519 secrets, the ed25519-compatible one, which stores `8·k` in place of `k`, so the same key has two byte forms. Accepting both layouts would be ambiguous: about one in eight byte strings in the ed25519-compatible layout is also a valid canonical key, but for a different key. A secret that is not a canonical key is refused, never reinterpreted. The nonce is not checked: a payee accepts any 32 nonce bytes, because a coin's nonce is not fixed by its derivation (A2).
6. Funding for a top-up arrives as a signing capability, not as key material, so the funding account's secrets never enter the layer. The capability does not survive a restart, so a top-up still waiting to be built gives up after a restart (C2 invariant 5). Nor does it outlive its top-up: `run_due` drops the capability of every top-up whose status is terminal, since nothing builds its loads again, and a capability can hold a key a product handed over.
7. **Every exported coin reports its export status** (B1) until the status is final. The export status is how a sender tells a payment that is still waiting apart from one that is lost, and reclaim is only worth attempting for a payment that is still waiting.

**Verified by** checking each primitive against the rules above and the components it points at; that each primitive returns exactly the errors and failures enumerated; and that every status, outcome and event above traces to a state the cited component defines.

---

#### G2 · Subscriptions and events

**Owns** nothing beyond the streams it serves, each with the last value it emitted (§2). **Band:** effectful.

G2 serves G1's four streams: `status(op)`, `balance(purse)`, `export_status(export)` and `events()`. Their signatures are in G1's API block.

- **`status`** emits the operation's current `StatusItem` at once, then each change. After the terminal item it closes (invariant 2).
- **`balance`** emits the purse's current `Balance` at once, then each new value. A value is re-evaluated whenever a record or an observation changes, at every `run_due` call, and on every new block F1 observes; it is emitted only when it differs from the last one emitted (invariant 1). The stream closes when the purse closes (G1 invariant 4).
- **`export_status`** emits every coin's current export status at once, then each change. It closes once every coin's status is final (G1 invariant 7).
- **`events`** emits `Resynced` first, then every later change as it is committed (invariant 5). It does not close while the layer runs.

**Invariants**
1. A balance stream **must not be driven by state changes alone.** A balance is computed from records, and some of its inputs change with the clock, for example an entry's dwell passing or a coin's lock expiring. So the balance is re-evaluated against the clock and emitted when its value changed, whether or not a record changed. Balances derived from events alone would miss these clock-driven changes. The layer has nothing that wakes it at a given instant (F2 invariant 2), so a change caused by the clock alone is emitted at the next `run_due` call or new block, not at the instant it happens.
2. **Events follow commits, and an event is never the only place an outcome exists.** An event reports a committed change and may be delivered more than once, so a subscriber must tolerate duplicates. A subscriber that is attached learns of a change promptly from the event; one that was not attached reads the same outcome from the records, which are never deleted (B4). So a stream that nobody was listening to loses nothing. An operation's status stream emits its terminal item exactly once and then closes.
3. **A provisional verdict that is later withdrawn appears as a status that goes backwards, and the surface must not hide it.** A caller shown "succeeded" and then "pending" is being told the truth. Keeping the first "succeeded" would report a success that may never happen.
4. The value streams (balance, operation status and export status) emit the current value when a subscriber subscribes. The event stream does not, because an event is a change, not a value.
5. Subscriptions do not survive a restart, and callers subscribe again. Every event stream starts with one `Resynced` event, and every event after it is a live change; a caller that needs the state at that point reads it from the value streams. Dropping a subscription never affects the layer.

**Verified by** checking that a clock advance with no record change still emits when a balance moved, and that a terminal receipt is observable by a subscriber attached before termination.

---

#### G3 · Initialization

**Owns**, in memory, what the running layer holds: the interfaces and the `ScanConfig` given to `start`, the runtime version of the latest check, the `ChainParams` from the last check that passed, and whether the latest check failed. The last two, and whether the start sequence has run, are kept in the `Running` handle that `start` receives, which C2's plug-ins read (C2). None of it is stored: `start` receives it again, and the check runs again, at every start. G3 is G1's `start`, and it re-checks the runtime when F1 observes a new version. **Band:** effectful.

**`start(env, config, scan, purse_backup, running)`** runs these steps, and returns the `Layer` only after the last one:

```
1. Validate   read the runtime description and the configured instance's record from the chain;
              A3 validate(runtime_description, instance_record, config)
              → on failure: StartError::UnsupportedRuntime { check }       (invariant 1)
2. Open       open the store; an undecodable store → StartError::UndecodableStore (invariant 1)
              on a fresh store, in one store transaction (B2 mint_installation): mint the installation, store env.policy,
              restore purse_backup if given (invariant 3); purse_backup is ignored otherwise
3. Sequence   delete every uncommitted disclosure mark and give up its export
              (B2 uncommitted, B2 release_disclosed, B4 record_release);
              open Running, so that prepare starts building                  (invariant 5)
4. Register   start the installation's registration, unless one succeeded or is open (invariant 4)
```

```rust
fn revalidate(at: BlockRef);   // F1 calls it when it observes a new runtime version at `at`
```

**`revalidate`** is the runtime check after an upgrade. It reads the runtime description and the instance record at `at` and calls A3's `validate`. While the check fails, every primitive that starts an operation fails with `UnsupportedRuntime`, and `prepare` answers no record (invariant 7). While the check fails, G3 keeps the `ChainParams` from the last check that passed, and every read that takes `ChainParams` uses them, so balances and classifications stay available during the upgrade. When a check passes, the new `ChainParams` replace the old ones for every later read and build.

**Invariants**
1. **Fails rather than degrades.** Each of these stops the layer from coming up: an unsupported runtime, a limit the runtime should publish but does not, or an undecodable local store. The runtime is checked by A3's `validate` at connection and again whenever F1 observes a new runtime version, not at the first rejected transaction, and while a check fails no operation starts (`UnsupportedRuntime`). The reason to refuse an undecodable store is not key reuse: treating the store as empty mints a fresh installation (invariant 3), and allocating in a namespace that nothing has used is safe. The reason is that everything else in the store would be silently discarded: transactions in flight, the rows holding their inputs, the marks on disclosed coins, and the purse list. If the layer came up anyway, a store that might still be repaired would be lost for good.
2. The main purse exists as soon as the seed is present. On first initialization no other purse exists, except the purses a metadata backup restores (invariant 3).
3. **A fresh installation identifier is minted whenever the store names no current installation**, and it becomes the only namespace this instance allocates into (B2 invariant 4). The same identifier is the installation the rest of the host derives keys under, such as NFT purse item keys: the layer owns it, registers it (invariant 4), and hands it out. Each identifier is a fresh 32-byte draw from §1's randomness, so no identifier is drawn twice (A2).

   A missing installation is the right trigger only because the store is never restored (§1). Under that obligation, a store that names no installation is a store that has never been written, so minting when the installation is missing is the same as minting whenever the store does not continue from an earlier state. Without the obligation, this trigger would not work: a restored store names an installation and carries counters, so no new installation is minted, and the instance moves its counters back and derives again items that the original instance already issued.

   On a fresh store, minting the installation, storing the initial policy (§1) and restoring a purse-metadata backup, if one exists, happen in one store transaction. So a crash cannot leave an installation without its backup, and allocation starts past every identifier the backup lists.
4. **The current installation is registered on chain** by an ordinary operation, not by a side channel of initialization: E3's `start` with `Request::Given(PlanInput::RegisterInstallation(id))`, under the layer's reserved scope, with the retry policy `ExpiryOnly`. F4 defines the registry, the record's sealing and the append call, and how the `//datastore` account pays for it (F4 invariant 4). At every start, after invariant 5's sequence, a registration is started unless one has already succeeded or is still open. So neither a crash before the registration started nor a failed registration leaves the installation unregistered.

   Allocation does not wait for the registration: the wallet is usable immediately. But until the registration lands, value in that installation can be found only through this store. So `recoverable()` reports the registration's progress (G1). The wallet is usable before it is recoverable, and the user needs to know which of the two holds.
5. **Start sequence.** It is the one ordering rule that the whole design assumes: construct → delete every uncommitted disclosure mark and give up the exports they belonged to (B2 invariant 2, C2 invariant 5) → open `Running` → *only then* accept a new operation. The middle step cannot be skipped: an uncommitted export left in place would mint coins whose secrets reached nobody, or would keep its inputs held forever. The core creates the durability layer with C2's plug-ins before this layer starts (§1), so the durability layer may already ask `prepare` to build while the sequence runs. `Running` is what holds building back: until G3 opens it, `prepare` answers no record (C2 invariant 6).
6. The local store is confidential. The storage layer beneath it encrypts it at rest (store contract, item 5); this layer does not. The store holds the seed or a handle to it, C1's rows, which are more sensitive than the asset records alone (C1 invariant 6), and imported secrets until their claims are final (G1 invariant 5). Nothing the store holds, and no key the layer derives above a coin or an entry, is written to a log or a diagnostic.
7. **A runtime upgrade neither strands nor corrupts registered work.** While the new runtime fails A3's check, no record is built and none is given up because of the check: `prepare` answers no record, so records wait, and their own deadlines and cancellations still apply. Attempts already in flight settle by their status. Once a check passes, every later attempt is built with the new `ChainParams`. Records store their call by name (E3) and signed bytes are never stored (§1), so a record registered before the upgrade is built against the new call and origin indices, never against stale ones.

**Verified by** attempting construction under each refusal condition; checking that no primitive is accepted before invariant 5's sequence completes; and, for invariant 7, failing and then passing the runtime check with changed call indices while records wait, and checking that no bytes are built in between and that the next bytes use the new indices.

---

## 5. Alignment with the shipped wallet

Some parts of this design are fixed rather than chosen, because the shipped mobile wallet, the Polkadot app for Android [Android app] and iOS [iOS app], already derives keys from users' seeds and holds real value under those keys. This section compares the design with that wallet.

Where the design departs from RFC-6 or RFC-17, [`rfc-deviations.md`](rfc-deviations.md) records each departure and the RFC amendment that would remove it.

**Bound by compatibility, so they cannot change without a migration:** two separate roots (`//coinage` and `//coinage-ring-vrf`); the main purse as `u32::MAX` rather than `0`; the installation segment as a 32-byte identifier; the coin item junction being soft while every other junction is hard; and the installation registry, meaning its contract, the `//datastore` account and the sealed record format (F4 invariant 4).

The soft coin item junction matters most, because it is a security property, not a naming choice. It was chosen deliberately, for speed: deriving a soft child from a cached parent is one cheap operation, and reconstruction derives thousands of coin indices per namespace. A2 states the condition under which the soft junction is safe, and A2's open question says plainly that this condition needs a cryptographer's judgement.

**Adopted because the wallet's solution is better**, not for compatibility:
- installation-scoped allocation, because a counter does not survive a reinstall, together with the installation registry published on chain, so that the seed alone is enough to find earlier namespaces;
- balance as a three-way partition, because overlapping totals hide which total applies;
- a policy interface rather than fixed thresholds;
- rebuilding into the same record, because a new record per attempt would release the inputs between attempts;
- caller-supplied operation identity;
- the rule that a missing or failed read never overwrites a stored value;
- **asset state derived, never stored** (B1), because a stored status drifts away from the transactions it summarises; the wallet first stored a status and later removed it;
- **a durability layer that runs the retry loop** and asks the domain for a decision and for bytes (§1, C2), so that no domain coordinates its own retries;
- **a handoff committed in the caller's own transaction** (G1 invariant 3), so that a payment message and its commit are durable together;
- **background execution while any transaction is open** (F2 invariant 2).

**Matched to the wallet's behaviour**, so that the same privacy setting does the same thing on every host: the three presets and their values (A4); readiness as alternative rules, with the dwell counted from the key's inclusion in the ring root (B1); no random delay on entries; holding back the coins the recycling sweep takes, and the sweep's cap, which counts value still minting and admits a coin while any headroom is left (B1 `recycle_due`); the age cutoff two below the runtime's maximum, with a configured fallback (A3); spending held-back value only as the policy allows, with the user's confirmation where it says so (E3, F3); the order of selection's strategies by who asked for the spend (B3); and a token reserve that only stops the recycling the policy asks for (D1).

**Where this design goes further than the wallet:** reclaiming an unclaimed payment, on request or after a claim deadline (C1 invariant 4); and holding an export's inputs from the moment the export starts, whereas the wallet only checks them at commit and rolls the message back if another payment took them.

**What matters for the Android wallet's durability code.** The Android wallet decides its coinage transactions with a completion oracle of its own (`CoinageResourceOracle` and `CoinageRules`) inside a durability engine shaped like the one in §1. Comparing it with C2 shows three points the wallet should check:
- **A coin-origin transaction can be replayed after a failed dispatch.** The wallet records a failed dispatch read in a finalized block as final at once, both when it follows an attempt and when it searches blocks. For a coin origin, the coin is returned and locked, and once the lock has expired the same bytes can be included again while their window is open (§1). If that later inclusion succeeds, the wallet has already freed the inputs and treats the outputs as never minted. This design has the same gap (C2, *Not here*).
- **A failed dispatch can be recorded as an expiry.** The wallet claims *not completed* whenever an input is available, without checking that no failed dispatch shows (C2). A token-origin attempt whose dispatch failed is then recorded as expired. The wallet loses no token by this, because each rebuild picks a fresh token from chain state, but its retry decision sees an expiry where the attempt reached the chain.
- **Records waiting to be built are not counted as consumers.** The wallet treats an output as untouched when no submitted record has it as an input. C2 also counts records waiting to be built. This matters only together with the first point: a waiting record whose earlier coin-origin attempt failed its dispatch can still have those bytes included again.

## 6. What this does not decide

- **The upper layer.** Everything above the export and import primitives.
- **Concurrency and ownership.** Whether the coinage layer is driven by one owning task or shared under a lock. C1's registration check prevents conflicting operations either way, and B2 invariant 3 is the only concurrency property anything depends on.
- **Module structure.** A component is a unit of specification and verification, not a code module.
- **How statuses are reached.** That is internal to the durability layer.
- **How soon a build follows an export's commit**, postponed. The engine asks `prepare` again only after a backoff of up to two minutes (§1), so a minting can start up to two minutes after its commit. Instead, `prepare` could wait briefly for the commit, or the engine could offer a call that wakes a group.
- **Storage growth.** Records and their rows are never deleted (C1 invariant 1), so the store grows with every transaction. Compacting old records would have to keep every fact that B1 and A2 still read.
- **Three obligations on the environment**: the shared store (§1), something that invokes the coinage layer periodically (F2 invariant 2), and a subscription surface for continuous observation (F1 invariant 6). The durability layer's background execution covers open transactions only; it does not cover the rings of a wallet with nothing in flight.
- **The layer runs only where the seed is.** It derives every key itself (A2), so it runs on the device that holds the seed: a signing host such as the phone or the command-line host. A host paired with that device runs no coinage layer of its own.
- **One live installation per seed.** A second device on the same seed sees the first device's installation in the registry as an earlier one, reconstructs it and spends its coins, and the two devices then compete on chain for the same coins. Running one seed on two devices at once is outside this design.
- **The app's own wallet runs on this layer.** A host app's wallet screens (the balance, sending, receiving, the chat's payments) call this layer through the core's native interface, which exposes G1's primitives and G2's subscriptions to the app, and the app runs no coinage engine of its own. Two engines on one seed derive the same keys and compete for the same coins, as two devices do (the item above). So a host turns coinage on in the core only once its wallet screens use that interface, and stops running its own engine in the same release. The shape of the native interface is not decided here.
- **Runtime features left for later.** The coinage pallet [coinage pallet] has features this design does not use, and allows situations this design assumes away. Each item below is a later extension:
  - **Several instances.** The layer serves one instance, fixed by configuration (A3). Serving several means purses that hold coins of several assets and units.
  - **Archived rings.** The chain archives an expired ring and lets its entries be unloaded later from the archive, for a fee. The design ignores this and treats an expired ring as destroying its entries (F2, P1), so it can only overstate the loss, never understate it.
  - **Other ways to pay for an unload.** The chain also sells unload tokens, for a coin, the native currency or the external asset, and lets an unload pay its fee from the value it unloads. The layer uses only the credential's free tokens (D1).
  - **Negative exponents.** Coins smaller than a cent exist on chain. The layer neither creates nor accepts them (A1).
  - **Direct offboarding.** The chain can turn a coin directly into the external asset, which links the coin's transfer history unless the coin is fresh. Offload always goes through a recycler (F3).
  - **Parameters changed at run time.** Governance can change the maximum coin age, the token allowances and the load deposit without a runtime upgrade. The design assumes they stay as read at connection.
  - **Batched loads.** The chain accepts several unpaid loads in one call. A top-up makes one load per transaction.
  - **Sponsored instances.** An instance whose loads take a deposit from its pot can refuse a load when the pot is short. The design assumes the configured instance takes no load deposit.
- **The core's top-up check, to replace.** Today's core runtime validates the secrets of an RFC-6 coin or private-key top-up with `sr25519_secret_from_bytes` (in [TrUAPI core], `rust/crates/truapi/src/host_internal/extrinsic.rs`), which tries the canonical layout and falls back to the ed25519-compatible one. That function was written for pairing keys, where a derived public key is checked against the expected one, and was reused for top-ups, where no such check exists. The implementation of this design replaces that check: a top-up's secrets are read in the canonical layout only (G1 invariant 5; layer 2, N2 invariant 4), and a secret in any other layout is refused with `InvalidSource` when the call is made. The pairing keys keep their fallback.

## 7. Composition: why the system properties hold

§4 states what each component guarantees locally. That is not the same as the system being correct: a property below can fail while every component is individually correct.

Each argument names the property, **lists the ways it could fail**, and shows which invariant excludes each way. The list is the important part. A reader who finds a failure mode missing from a list has found a gap in the argument, even if every cited invariant is correct.

---

### P1 · No value is silently lost

*No chain behaviour, host behaviour or interruption makes value the user controlled unreachable unless the user chooses it.*

| How it could fail | Excluded by |
|---|---|
| A coin reaches the age cutoff | Not a loss: the coin can still be recycled (§1), and the sweep recycles every coin at the cutoff whatever the cap or the reserve says (F2 invariant 6; A4's `effective` keeps `recycle_at` at most the cutoff) |
| An entry's ring expires and the chain destroys its backing value | F2 invariant 1, and F2 invariant 6, which exempts the rescue sweep from the availability cap |
| The rescue waits for a token and its ring expires first | A3 invariant 3: the rescue margin exceeds a token period plus a validity window, so a rescue can wait for the next period. A period with no free token left for a rescue is P10's residual |
| An entry that an ordinary operation holds reaches its ring's expiry | C2 invariant 5: the ordinary record gives up at the rescue margin, which frees the entry for the rescue sweep (B1 invariant 2) |
| Neither sweep ever runs, because nothing invokes the layer | **Not excluded.** F2 invariant 2: while the layer runs and is subscribed, each new block invokes it (`Event::NewBlock`); otherwise invoking the layer is a host obligation, and F2 invariant 2 names the consequence |
| A sweep runs but sees nothing, because observation never supplied the deadline or the age | F2 invariant 5: *cannot tell* is a third outcome, separate from *nothing due* |
| An asset is treated as spent while the chain still holds it | B1 invariant 2, F1 invariant 2 |
| Durable state is lost and the namespaces cannot be found again | F4 invariant 4: the installation registry is published on chain. A2 invariant 1 makes derivation reproducible |
| After a wipe nobody starts a reconstruction, and forgotten entries expire | F4 invariant 5 and F2 invariant 3: the entry point starts the default reconstruction itself when it is due |
| The store is lost before the registry write landed, so that namespace cannot be found | **Not excluded.** G3 invariant 4 reports the registration's progress, so the user learns that the wallet is not yet recoverable. Nothing guarantees that the registration lands |
| A per-network limit is silently given a default, so a deadline is wrong | A3 invariant 2: an unpublished limit is never given a default. The limits that guard a ring's expiry are among those limits. The age cutoff alone falls back to a configured value; P12 states what that risks |
| An installation is registered but never scanned, and reads as holding nothing | F4 invariant 5: scan progress is tracked per installation, and an installation is not done until its first scan completes |
| A reconstruction stops early and reports a short result as a complete one | **Not excluded.** F4 makes the scan's stopping point explicit and offers a deeper scan, which makes a missed asset less likely without excluding it |
| Value under a product purse identifier is not found | **Not excluded.** F4: purse identifiers can be enumerated, but each costs a full scan, so they are walked on request rather than by default |
| A reclaim cancels a coin's minting and strands its inputs | C2 invariant 5: the minting is given up at a build, when no attempt is in flight, so nothing can still land, and its inputs are free again (B1 invariant 2) |
| Value lands in a purse that has since been closed, so no balance counts it | G1 invariant 4: once a deletion is accepted, no new operation may name the purse as its destination, and the purse closes only when no open operation still lands value there. C1 invariant 4: a reclaim must name an open purse. B2 invariant 7: a deletion's target is open when the deletion begins, so `resolve_purse` always ends at an open purse |
| A runtime upgrade fails A3's check for longer than an expiring ring's rescue margin | **Not excluded.** G3 invariant 7: while the check fails, no record is built, so a due rescue waits; F2 invariant 5 reports it as *cannot tell*. If no runtime that passes the check arrives before the ring expires, the entries in it are lost |
| An operation strands its inputs by never terminating | P8 |
| Value is spent to an account the layer then forgets | C1 invariant 1 with E3 invariant 2 |
| A purse is closed while value is still in it | G1 invariant 4: closing discards the purse's records, so the purse closes only after the chain confirms the value has left, and a purse holding value that cannot move is refused |
| The layer itself builds a transaction that destroys value | A5 contract 1: each split conserves value. E1 invariant 3: each unload group's outputs equal its inputs, and the group's own change takes the remainder |
| A transaction submitted before a wipe lands after the reconstruction | F4 invariant 5: the default reconstruction starts only after every such transaction has landed or died |
| An export's minting lands although its secrets reached nobody | G1 invariant 3: the commit is atomic with the caller's message. C2 invariant 5: nothing is built before the commit |

**Five residuals, and they are of two kinds.** The first kind is an external dependency, and two rows are of this kind. The row in which nothing invokes the layer is the layer's largest external dependency: a host that never invokes the layer loses value once a ring expires, and no invariant here prevents that. The row about a runtime upgrade that fails A3's check is similar: the layer refuses to build against a runtime it cannot check, so it depends on an implementation that supports the new runtime arriving before a ring's rescue margin runs out.

The other three residuals follow from the rule that the store is never restored (§1). They are the rows about the namespace whose registration never landed, the reconstruction that stops early, and the product purse identifier that is not found. In each of them, value exists on chain and can be reached with the seed, but a reconstruction does not *find* it. None of them destroys a coin. All except the unregistered namespace stay recoverable by a holder who knows where to look, and each is stated so that a holder can look. Two cases are worse: an entry that is not found is destroyed when its ring expires, and a namespace whose registration never landed cannot be found at all. The design accepted these residuals instead of the alternatives, which F4 describes: publishing high-water marks reveals a transaction count, and publishing purse identifiers links a seed to its products.

---

### P2 · No input is consumed twice

*Two operations never spend the same coin or entry, and one operation never spends it twice.*

| How it could fail | Excluded by |
|---|---|
| Two operations select the same asset | B1 classifies an input of a transaction that has not failed as `InUse`; C1 invariant 2 refuses a second consumer at registration |
| An asset is selected and its registration then fails | E3 invariant 1: selection runs inside the registration's store transaction, and C1 checks every input again there; a refusal rolls the whole registration back |
| A coin exported as it is, which no registration holds, is selected by another operation | B2 invariant 2: its mark is checked and written in one store transaction |
| A reconstruction adds a second row for an asset the store already holds | F4: a scan adopts only indices B2 does not already hold (B2 `adopt`) |
| A crash between selecting and registering | E3 invariant 1: selection and registration are one store transaction, which commits completely or not at all |
| A restart forgets which assets are held | B1 invariant 2: the hold is the registration itself, which is durable |
| Two attempts of one operation both take effect | C2 invariant 5: both attempts have the same inputs, so the chain refuses the second |
| Two attempts are live at the same time, and each uses up a token | §1: at most one attempt of a record is live at a time |
| A reclaim and the payee's claim both take the same exported coin | The chain moves the coin once; the side that moves second fails as sniped (C1 invariant 4) |
| A reclaim gives up a minting whose transaction then lands anyway | C2 invariant 5: giving up happens only at a build, when no attempt is in flight; an attempt in flight is waited for instead (B4) |
| A cancellation arrives while an attempt is being built | B4 invariant 1: the attempt may still be submitted, and the operation ends according to its status; its inputs stay held while it is in flight (B1 invariant 2) |
| An input is freed while its transaction can still land | B1 invariant 2 |
| A caller repeats a call after a crash, and a second operation spends the same value again | B4 invariant 5: B4's `begin` joins the operation the key already names, or returns its completion; the same key with a different request fails with `KeyReused` |
| An export is committed after it was released, when its coins are free again and may be selected by another operation | G1 `commit_export`: an export that was released or has ended fails with `ExportClosed` |

**No residual.** The argument does not depend on the durability layer being correct in general, only on it never reporting a false stable status, which is part of its contract (§1).

---

### P3 · Crash safety

*An interruption at any point leaves state from which the layer recovers to a state consistent with what actually happened on chain.*

The layer changes durable state in few places, and each change is one store transaction. Everything that happens after registration is handled by the durability layer, whose own records survive a crash.

| Interruption | What is left | Recovery |
|---|---|---|
| Before registration | Nothing | Nothing to do |
| During registration | Nothing, or the complete records and C1's rows | One store transaction (C1 invariant 1) |
| After an export registered, before its commit | Uncommitted marks; records waiting for the commit | G3 invariant 5 deletes the marks and gives the export up; nothing was built (C2 invariant 5) |
| During the commit | Nothing, or the commit together with the caller's message | One store transaction (G1 invariant 3) |
| During the first start | Nothing, or the installation together with the initial policy and the restored backup | One store transaction (G3 invariant 3) |
| While an attempt is being built | Nothing durable | The durability layer asks `prepare` again, which rebuilds the transaction from C1's rows and the call stored with the record (C2 `prepare`); the bytes are not stored, since the plug-ins never write (C2 invariant 4) |
| While an attempt is submitted | The record, *submitted* | The durability layer settles it, and the layer reads the outcome |
| While a status is being written | Handled by the durability layer | A stable status never changes once written (§1) |

Four things outside that sequence also survive an interruption: a sweep keeps no schedule state, so there is none to lose (F2 invariant 3); an offload's phase is read from its records, and its wait and end are stored in B4, so the offload resumes at the phase its records show (F3 invariant 4); a reconstruction resumes from the scan progress B2 stores (F4 invariant 5); and the plug-ins cannot be in the middle of a write when the process dies, because they never write (C2 invariant 4).

**No residual**, provided the store offers serializable transactions. §1 lists the store as an external interface, so this is a stated requirement on the store, not a hidden assumption about the layer.

---

### P4 · Determinism and conformance

*Two implementations given the same seed, chain, clock, policy and random draws produce the same on-chain effects and the same records.*

| Where they could diverge | Pinned by |
|---|---|
| Which account an item names | A2 invariant 1 |
| How an amount decomposes into denominations | A1 `denominations`: the one decomposition with the fewest coins |
| Which records selection consumes | B3 invariant 2, and B3 invariant 1: readiness is computed, not stored |
| How coins are reshaped | A5 contract 5: one plan, defined by the runs of forced exponents, given the counts that B3 invariant 2 fixes |
| How a selection becomes transactions | E1 invariant 5 |
| Which of one account's signed transactions is built first | C2 invariant 5: registration order |
| What the sweeps start, and the ids of the layer's own operations | F2 invariant 3 |
| Which unload tokens are spent | D1 invariant 3 |
| The policy values a component works with | A4 `effective`: every reader applies the outer limits to the stored policy in the same way |
| The validity window and the bytes of an attempt | C2 `prepare`: the period is A3's longest validity window, halved until the window ends before an `Until` deadline, and the birth block follows from the block the context is read at. E2 `assemble` is a function of its arguments (E2 invariant 4) |
| The order transactions are submitted in | E1 invariants 1–2: independent transactions have no order to disagree about. Dependencies arise only across operations, through a coin still being minted, and those dependencies fix the order |
| The installation identifier | An input, not a choice (§1) |

**Residual.** Conformance holds only for the same randomness sequence and the same clock, both listed in §1. Two implementations that read the chain at different instants may legitimately differ; the claim covers identical inputs only.

---

### P5 · No derived key is used twice

*No key the layer derives is allocated to a second use, under any sequence of crashes, reinstallations or reconstructions; and no value from before a reinstall becomes spendable by any purse except the main purse.*

The property is about keys, because A2's reason for forbidding reuse applies to keys. Purse identifiers may recur across installations, because the caller keeps them. A recurring purse identifier is safe for keys because the installation sits below the purse in every derivation path, and B2 invariant 5 makes it safe for value.

| How it could fail | Excluded by |
|---|---|
| A counter is reused within a running instance | B2 invariant 4 |
| A record is deleted and its index allocated again | C1 invariant 1 |
| A counter goes backwards while the store still loads | B2 invariant 4: every field that records what was used is append-only |
| The store fails to load and is treated as empty | G3 invariant 1 refuses to start. Key reuse would not happen here: a store treated as empty mints a fresh installation, so the risk is lost value (P1), not a reused key |
| **The store is lost and allocation restarts at zero** | G3 invariant 3: a fresh installation is minted, and B2 invariant 4 confines allocation to it, so the item counter's value no longer matters |
| Two installations collide | G3 invariant 3: each identifier is a fresh 32-byte draw from §1's randomness |
| Reconstruction derives new keys in an earlier namespace | B2 invariant 4 and F4: earlier installations are read, never allocated into, and what is found there never advances a counter |
| A caller presents a purse identifier from before a reinstall | Not a failure. The identifier recurs, but the keys do not, because every path has the installation below the purse (A2) |
| A purse identifier that held value before a wipe is handed to a different caller | Harmless. B2 invariant 5: a purse holds only the current installation's coins, so the earlier value counts toward the main purse instead |
| A purse restored from the metadata backup spends value from before the reinstall | B2 invariant 5: the purse comes back with its owner and empty; the earlier value counts toward the main purse |
| A restored metadata backup and a new purse claim the same identifier | G3 invariant 3: the backup is restored before the first `create_purse`, and B2 allocates past every identifier the backup lists, closed purses included (B2 `purse_backup`) |
| **The store is restored from a backup**, so it names an installation already allocated into | §1's obligation on the host. The layer cannot exclude this case, because a restored store is byte-identical to one the layer wrote |

**No residual inside the layer, one obligation outside it.** The obligation is §1's rule that the store is never restored. No invariant can detect a restored store, because any marker the layer writes to recognise one is restored with it. A reinstall without a usable metadata backup does lose which caller a purse belonged to, but that is outside this property: no key recurs, no value changes hands, and the assets stay with the user in the main purse.

---

### P6 · Key material does not escape

*No key derived from the seed reaches the caller except as the declared output of an export, and no exported coin is later treated as owned value.*

| How it could fail | Excluded by |
|---|---|
| A primitive returns a key | G1 invariant 5: export is the only path |
| A secret is released for a coin that a reorganisation removes | Permitted and harmless. E3 invariant 4: the secret controls only its output account, which a later attempt creates at the same index, or which is never used |
| An export's secrets reach the caller but never leave the host | G1 invariant 3: until the caller commits, a restart returns the coins |
| The caller sends the secrets and the layer still treats the coins as its own | G1 invariant 3: the commit is part of the caller's own transaction. A caller on another store commits no later than it makes the secrets durable |
| An exported coin is spent again by the layer | C1 invariant 4: the disclosure mark. Its one exception, reclaim, moves the coin to an account whose secret was never disclosed, which ends the payee's control instead of sharing it |
| **Reconstruction re-adopts an exported coin**, because the mark is local state | Not excluded. See the residual below |
| The installation node's public key leaks, so a disclosed soft-derived coin secret lets someone recover the parent secret | G1 invariant 5: no primitive returns it. G3 invariant 6: nothing logs it. F4 invariant 4: the registry publishes only the installation identifier |
| The store discloses keys or links | G3 invariant 6 with C1 invariant 6 |
| Key material enters the layer and is kept | G1 invariant 6: top-up funding arrives as a signing capability, never as a secret. G1 invariant 5: an imported secret is kept only until its claim is final |

**Two residuals.** First, the soft coin junction is safe only if the installation node's public key stays private (A2), and A2's open question records that this rests on a reading of the cryptography, not on a proof. It is the one place where this argument ends in something unproved. Second, a reconstruction after a wipe cannot know which coins were exported, because the disclosure mark is local and publishing it would name those coins (F4). Such a wallet overstates its balance and may select value that a third party can also spend. Both residuals are stated; neither is closed.

---

### P7 · The layer does not degrade its own anonymity

*Nothing the layer stores, reports or does reveals a link that the recycler hides.*

| How it could fail | Excluded by |
|---|---|
| The durable record shows which inputs produced which outputs | C1 invariant 6: the rows hold indices, and each attempt's hash, kept beside them, is protected by the store's encryption at rest (G3 invariant 6) |
| A completion links an unload's outputs back to the entry it consumed | B4: a completion names the coins a transaction consumed and created, never a recycler entry |
| An offload's completion links the coins it recycled to its destination | B4: an offload's completion lists only the offboarding transactions |
| One transaction mints into two purses, linking them | E1 invariant 3 |
| Diagnostics or logs reveal that link | G3 invariant 6 |
| A key is reused, linking new activity to old | P5 |
| A load is linked to its unload by timing | A4's dwell, as far as the policy sets one; the layer adds no random delay, as the shipped wallet adds none |
| A reconstructed entry loses its timers and becomes spendable immediately | F4 invariant 3: a missing inclusion instant reads as *now*, so the dwell is served again instead of being treated as satisfied |
| An entry is spent from a ring too small to hide it | A4's threshold, applied in B1 invariant 3, reaches selection as a classification. B1 makes `Degraded` entries unselectable, not merely labelled |
| A product decides to spend entries below that threshold | B1: only the user's policy can allow it; no primitive accepts a caller's request for it |
| The rescue sweep's exemption from the availability cap is used as a general bypass | F2: the exemption applies only to the rescue sweep, which unloads entries only into fresh coins of the purse they belong to, never to a payee |
| Offload surplus lands as a fresh coin, linking the entry set again | F3 invariant 3, which A3 invariant 3's per-load cap always makes possible |
| An offload silently uses a weak anonymity set | B1: only if the user's policy allows Degraded entries, never because a product asks |
| Recycling a coin only to spend it again gains nothing but looks private | B3 invariant 2: a spend for a product takes entries, which are already private, before coins |
| A product makes the layer spend value that is still gaining privacy | A4: held-back value is spent only as the user's policy allows, and `AfterConfirmation` asks the user first (E3, F3, G1 `NeedsConfirmation`); a product can only ask |
| A move between purses links the two purses' coins on chain | A4 `move_through_recycler`: `rebalance` and `delete_purse` move value through the recycler when the user's policy says so (E1 `plan`); a direct move is the user's choice, never a product's |

**Residual.** The layer's own behaviour is covered. Correlation by an observer who analyses the whole chain is out of scope.

---

### P8 · Every operation terminates

*No operation stays open indefinitely while holding inputs the user cannot recover.*

| How it could fail | Excluded by |
|---|---|
| A transaction is neither included nor expired | E2 invariant 3: every transaction is mortal, which bounds the durability layer's search |
| A status never becomes stable | The durability layer's contract (§1): every transaction is mortal, so a transaction that cannot take effect is eventually judged failed |
| The retry policy rebuilds forever | C2 invariant 3: a failure that would repeat on the same inputs is not retried indefinitely. Repeated expiry is bounded by a deadline under `Until`, and under `ExpiryOnly` only by a cancellation (E3, B4 invariant 1); see the residual |
| An export is never committed and holds its inputs | G1 invariant 3: `release_export` releases it, and a restart gives it up (G3 invariant 5) |
| A multi-phase offload loops forever | F3 invariant 1: each Plan re-reads the chain, so the loop is driven by changing chain state. Between phases the offload is `Waiting`, with its wait stored in B4 (B4 `record_offload_wait`), so it stays cancellable (B4 invariant 1) |
| An operation is stuck because a dependency is stuck | C1 invariant 5: the dependency graph is acyclic, so termination propagates from the transactions that depend on nothing |
| An operation waits for inputs that never become usable, such as a ring that never fills | B4 invariant 1: `Waiting` is cancellable, so the caller can decide to stop waiting |
| A reclaim waits forever for a coin whose minting never progresses | C2 invariant 5: a minting with no attempt in flight is given up at its next build, and an attempt in flight ends when its mortal transaction does (E2 invariant 3) |
| A top-up waits for a funding signature that a restart took away | C2 invariant 5: it gives up (G1 invariant 6) |
| A reshaping payment builds a chain of dependent transactions that must run one after another | A5 contract 3: no split consumes another split's output, so the plan submits in one round and has no chain to stall |
| No verdict can be reached because an output cannot be attributed to one operation | C1 invariant 3: every output identifier is fresh, which is what lets chain state answer the question at all |
| A runtime upgrade fails A3's check, and registered records wait | G3 invariant 7: `prepare` answers no record, but each record's own deadline and its operation's cancellation still apply, and an attempt in flight settles by its status. An operation under `ExpiryOnly` waits until a check passes; see the residual |
| A rescue keeps failing until its ring has expired | F2: a rescue runs under `Until(the ring's expiry)`, so it ends at that time at the latest |

**Two residuals.** First, termination depends on the host invoking the coinage layer, the same dependency as P1's row in which nothing invokes the layer. An operation in `Waiting` on a host that never invokes the layer does not progress, though it stays cancellable. Second, an operation under `ExpiryOnly` whose caller never cancels it stays open, holding its inputs, for as long as the chain keeps not including a valid transaction, or while a new runtime fails A3's check (G3 invariant 7). Layer 2's flows all run until a deadline, so among callers only a direct caller of this layer is exposed. The layer's own recycling is not exposed, because it runs `Until` one token period after it starts (F2 invariant 3). The layer's own installation registration also runs under `ExpiryOnly`, but it holds no coin or entry.

---

### P9 · The balance never overstates controlled value

*A displayed spendable figure never exceeds what the user can actually spend.*

| How it could fail | Excluded by |
|---|---|
| A lagging read zeroes or inflates a holding | B2 invariant 6 |
| A never-observed record counts as spendable | B1 invariant 1 and B2's partition: it counts as *arriving* |
| An entry not yet included in its ring root, or in a degraded ring, counts as spendable | B1 invariant 4 and B1 `classify`: an entry not yet included counts as *arriving*, and a `Degraded` entry counts as *held back* |
| A reserved or in-flight record counts as spendable | B2's three-way partition |
| An input and the outputs it becomes are both counted | B2: an input is in no bucket; only the operation's outputs count |
| Entries count as spendable although no token can unload them this period | B2: an entry is spendable only while this period's free tokens can unload it |
| An expired entry still counts | B1: `Expired` counts in no bucket |
| An import counts a coin at the value its sender declared | B1: a peer's coin counts nowhere, and the import's output counts at the exponent observed |
| A chain-locked coin is shown as spendable | B2's partition places it in *arriving* |
| A record becomes spendable as time passes, and the displayed figure goes stale | G2 invariant 1 |
| A threshold is evaluated against a stale classification | B1 invariant 3: readiness and selectability are functions of the current time, never cached |
| A holding is counted at one block height and its spend at another | F1 invariant 5: every `Facts` value is read at one block, and B2 `observe` stores it in one store transaction |
| A failed read resolves optimistically | B2 invariant 6: F1 reports it as `Read::Failed`, and the last stored value stands, neither the optimistic nor the pessimistic value |
| A negative exponent is truncated rather than refused | A1 invariant 1: A1 `exponent` refuses it when a coin value is read, because truncating misstates the balance. An import leaves such a coin where it is (G1 `import`) |
| Reconstruction adopts coins the layer does not solely control | Not excluded. This is P6's residual: the overstatement is toward *spendable* |

**One residual**, and it is P6's: a reconstruction that re-adopts an exported coin counts it as spendable. Otherwise any error is in the safe direction: the partition may *understate* the balance while a read is lagging. B2 accepts this deliberately, because a spend must lower the displayed figure at once.

---

### P10 · Unload-token accounting is sound

*No token is spent twice, and no allowance is left unusable.*

| How it could fail | Excluded by |
|---|---|
| Two operations present the same token | D1 `resolve` takes only free slots, E1 invariant 5 resolves once per operation, and C1 invariant 2 refuses a second holder of a token identity at registration |
| A token already consumed on chain is presented | D1's application state: `unknown` is treated like `applied` |
| A partial plan spends tokens and then fails | D1 invariant 1 |
| A retry silently uses up the allowance | C2 invariant 3: no retry once a token-presenting attempt reached the chain. §1 says what the lost token costs |
| A token's use is recorded separately from the chain's view, and the two disagree | E2 invariant 2: assembly writes nothing; whether a token was spent is read from the attempts and from F1's observation (D1) |
| A token spent outside this store, before a wipe or by another device, is presented again | D1: F1's observation marks it `applied` |
| A rebuild presents a token that its failed attempt already spent | C2 invariant 3: no retry once an attempt reached the chain |
| Recycling creates entries that no remaining token can unload | D1 invariant 2: while the free tokens are no more than the reserve, the recycling sweep admits no coin |

**One residual, and it is about capacity, not accounting.** Every operation may take any free token, so the rescue sweep is guaranteed none (D1 invariant 2). If the user's own unloads take a period's tokens, or more rings reach their rescue margin within one period than free tokens remain, the remaining rescues wait for the next period. A3 invariant 3 gives each rescue enough time for one such wait. So if rings keep reaching their margin while the tokens are used up, entries can be lost. F2 reports such a rescue as *cannot tell* (F2 invariant 5), so the loss is never silent.

---

### P11 · No outcome is reported as more settled than it is

*Nothing the layer tells a caller, whether a status, a receipt or a released secret, claims more finality than the verdict behind it supports.*

Several components each enforce part of this property. Stating it as one property makes them follow one rule instead of separate conventions.

| How it could fail | Excluded by |
|---|---|
| A provisional success is kept on display, so a later withdrawal is hidden | G2 invariant 3: the regression is shown, because it is what actually happened |
| An asset is classified spent, or an input freed, on a provisional status | B1 invariant 2 |
| The oracle concludes completion from an absence that has another explanation | C2 `proven_completed`: an absence counts only for settled inputs (rule 3), and an unknown read proves nothing (invariant 2) |
| The oracle concludes that a transaction did not take effect, although it did | C2 invariant 2: *not completed* rests only on facts that cannot reverse, never on the absence of an entry, an alias state or a token's consumption |
| A failed dispatch is recorded as an expiry, so a spent token counts as unused | C2 invariant 3: the oracle claims *not completed* only when no failed dispatch shows |
| `Settling` is treated as terminal | B4 invariant 2: provisional and never final |
| A receipt cites a block that can still be reorganised | B4 invariant 4: only stable outcomes, and only finalized block references |
| A secret released before its coin is stable is read as payment made | B1: the coin reports *not yet minted*, and B4 keeps the export open until a stable verdict |
| An export reports a claiming transaction that a reorganisation replaces | B1: `Claimed` and its transaction are final only once the move is stable; the same holds for `Reclaimed` |
| The surface invents a status the layer cannot justify | G1 invariant 2: the surface has no status vocabulary of its own |
| The layer concludes from chain state that a transaction of its own happened | F1 invariant 1: an empty account means "spent" or "not landed yet", and nothing observable tells the two apart. F1 invariant 3: only the durability layer answers that question |
| A bound is reached and reported as a failure | C2 invariant 5: reaching a wait is not a verdict; the record stays waiting to be built |
| A dependent is built on a predecessor's provisional status | C2 invariant 1: permitted only where the dependent's own success would prove that the predecessor executed on the same chain |

**One residual, and it is deliberate.** E3 invariant 4 releases coin secrets as soon as the registration naming their accounts is durable, so a payment can be sent without waiting for proving or finality. A payee may then hold a key to a coin that never comes into existence. This is not an overstatement as long as nothing reads the release as success, and B1 and B4 ensure that: the coin reports *not yet minted*, and the export does not settle early. Every other row of the table stays excluded: releasing early marks nothing spent and frees no input.

---

### P12 · The layer does not build transactions the chain will refuse

*A transaction the layer constructs is one the runtime accepts, except for conditions the layer cannot observe when it builds.*

This property is about more than efficiency. §1 says what a refusal costs: a token origin loses the token and nothing is restored. So a layer that builds transactions the chain refuses wastes the token allowance, and P10 and P1 are affected.

| How it could fail | Excluded by |
|---|---|
| A runtime change silently points a call or an origin at something else | A3 invariant 1: calls and origins are looked up by name, never by position, and a missing name fails `validate` |
| A record registered before a runtime upgrade is built with stale call or origin indices | G3 invariant 7: a record stores its call by name (E3) and signed bytes are never stored (§1), so every attempt after the upgrade is built with the new `ChainParams`. While the new runtime fails the check, `prepare` returns `NotYet` |
| An incompatible runtime is discovered only at the first rejection | G3 invariant 1: the runtime is checked at connection and again whenever F1 observes a new runtime version (G3 `revalidate`), and nothing starts while a check fails. A3 invariant 3: the check includes the relations between limits |
| A membership proof is built against the wrong ring revision | E1 invariant 3: entries are grouped by ring index *and* revision |
| A coin at the age cutoff is split, or exported for a claim the chain will refuse | B1: selectability requires an age below the age cutoff, also for coins a split mints for an export. The cutoff is two below the runtime's maximum coin age. **Residual:** when the maximum cannot be read, the cutoff is the configured fallback (A3), and on a runtime whose maximum is less than two above the fallback the chain refuses such transactions |
| A membership proof is built for a key not yet included in the ring root | B1 invariant 4: the included count is compared against the paged position, not against the position alone |
| A transaction exceeds a per-transaction chain cap | E1 invariant 4: caps are enforced before anything is derived, and an over-cap unload group is split across transactions |
| An offboard leaves its destination below the external asset's minimum balance | F3 invariant 5 |
| An origin's specific rules are applied at one call site and missed at another | E2 invariant 1: one construction path per origin |
| Two transactions of one signing account use the same nonce | C2 invariant 5: one transaction at a time per account. E2 invariant 4: each reads the nonce after the previous transaction is stable |
| The declared validity window disagrees with the encoded one | E2 invariant 3, and C2 `prepare`, which returns the window encoded in the bytes together with the bytes (`Build::Ready`). A disagreement would also make the durability layer answer about a different transaction |
| A split plan needs a split of a split, which exceeds what one transaction can do | A5 contract 3: no split consumes another split's output. Contract 4: outputs per split stay under the chain's cap, which relies on A3 invariant 3 |

**Residual, and it is why the property says "except".** The chain can refuse for conditions that did not exist when the bytes were built, such as a race with another spender or a changed fee. The retry policy accounts for that (E3), and §1 tells it what a refusal costs for each origin. The claim is that the layer adds no refusals of its own, not that refusals cannot happen.

---

### P13 · A caller can always find out what happened

*Every operation's outcome can be found by the caller that started it, however the caller was interrupted in the meantime.*

P11 is about soundness of reporting: nothing is claimed that the verdict does not support. P13 is about completeness: the outcome exists somewhere the caller can reach, not only in a notification that the caller had to be present to receive.

| How it could fail | Excluded by |
|---|---|
| The caller was not subscribed when the terminal event was emitted | B4's completion, read from records that are never deleted. G2 invariant 2: the event is never the only copy |
| The caller crashed and cannot tell whether it ever called | B4 invariant 5: re-issuing the same pair returns the completion instead of executing a second operation |
| A caller reuses a key for a different request and reads another operation's outcome as its own | B4 `begin`: a key that names an operation of a different kind or with different arguments fails with `KeyReused` |
| A caller's key collides with one of the layer's own operations | B4 and G1: the layer's own operations use the empty scope, and G1 refuses it from a caller (`ReservedScope`) |
| A retried commit, release or cancellation is reported as a failure, although the first call took effect | G1: committing or releasing an export again returns `Ok`, and cancelling an operation that has already ended returns `Ok` and changes nothing |
| An event is missed, or delivered twice | G2 invariant 2: events follow commits, a subscriber absorbs a duplicate, and a missed event loses nothing, because the facts it reports are still stored |
| A status stream never terminates, so the caller waits forever | G2 invariant 2: the terminal item is emitted exactly once, and then the stream closes |
| A failure to start cannot be told apart from a failure of a started operation | G1 invariant 1: a failure to start is an error from the call; a failure of a started operation is a terminal item on the stream |
| A partly successful operation is reported as a plain failure | B4 invariant 3: `PartiallyDone(amount)`, with the amount read back from the records |
| Subscriptions are lost at restart with no way to resynchronise | G2 invariant 5: callers re-subscribe, and every event stream starts with one `Resynced` event, after which every event is a live change. G2 invariant 4: the value streams send the current value when subscribed; the event stream does not, which is why the completion exists |
| The caller is told a selection failed but not what to do | B3 invariant 4: selection fails only for lack of value or of unload tokens, and G1's error says which: `InsufficientFunds` (the purse needs funding) or `NoUnloadToken` (a later period's tokens are needed) |

**No residual.** Records are never deleted (C1 invariant 1), so an outcome stays readable for as long as the store exists. The cost is a store that grows with every transaction (§6).

---

### What this section does not establish

1. **It is an argument, not a proof.** The lists are a human judgement about what can go wrong. Anyone can refute one by naming a failure mode it misses.
2. **Ten residuals are substantive.** Each is named at its property, and they fall into four kinds.

   *Obligations on the environment*: the host must invoke the layer (P1, P8), a direct caller using `ExpiryOnly` must cancel what it no longer wants (P8), and each period's free tokens must cover the rescues that fall due in it, after the user's own unloads (P10). No invariant can enforce these. A new runtime must also be supported before a ring's rescue margin runs out, because the layer builds nothing while the runtime fails A3's check (P1, G3 invariant 7).

   *Consequences of the store never being restored* (§1): a reconstruction may not find every namespace, may stop too early, and may not recover a product purse (P1); it cannot know which coins were exported (P6, inherited by P9); and a purse without a usable metadata backup does not come back at all (§1). The assets of such a purse still count toward the main purse (B2 invariant 5).

   *Trade-offs taken knowingly*: coin secrets are released before their coins are stable (P11), and the chain may refuse a transaction for conditions that arose after it was built (P12). Both are bounded, and the properties name what bounds them.

   *One unproved claim*: the cryptographic argument behind A2's soft coin junction, on which P6 relies. It is the only residual here that is not a knowing trade-off: it is an argument without a proof, and this document records no review of it (A2's open question).
3. **The durability layer's own correctness is assumed**, as §1 says: its guarantees on statuses, at its boundary. Every argument that cites a stable status relies on those guarantees; none relies on how the durability layer reaches a status.

## 8. References

Each entry names a specification, or, where no specification exists, the implementation that defines the behaviour, with the exact version or commit this design was checked against.

*Specifications.*
- [BIP-39] Mnemonic code for generating deterministic keys: https://github.com/bitcoin/bips/blob/master/bip-0039.mediawiki
- [FIPS 180-4] Secure Hash Standard, SHA-512: https://csrc.nist.gov/pubs/fips/180-4/upd1/final
- [Merlin] Merlin transcripts, built on STROBE-128: https://merlin.cool
- [RFC 7693] The BLAKE2 cryptographic hash: https://www.rfc-editor.org/rfc/rfc7693
- [RFC 8018] PKCS #5 v2.1, PBKDF2: https://www.rfc-editor.org/rfc/rfc8018
- [RFC 8032] Edwards-Curve Digital Signature Algorithm (EdDSA), section 5.1.5: https://www.rfc-editor.org/rfc/rfc8032#section-5.1.5
- [RFC 8439] ChaCha20 and Poly1305 for IETF protocols: https://www.rfc-editor.org/rfc/rfc8439
- [RFC 9496] The ristretto255 and decaf448 groups: https://www.rfc-editor.org/rfc/rfc9496
- [SCALE] The SCALE codec, in the Polkadot Protocol Specification: https://spec.polkadot.network/id-cryptography-encoding#sect-scale-codec
- [RFC-6], [RFC-17], [RFC-0010], [RFC-0021] The TrUAPI RFCs with those numbers, in https://github.com/paritytech/host-rust-core under `docs/rfcs/`: `0006-payments.md`, `0017-coinage-payment.md`, `0010-allowance.md` and `0021-payment-topup-coins.md`

*Implementations.*
- [AccountDataStore] The installation registry contract, as the Android app calls it: `feature/coinage/impl/src/main/java/io/paritytech/polkadotapp/feature_coinage_impl/data/dataStore/AccountDataStoreContractCoder.kt` in [Android app]. The app reads the contract's address from its remote configuration key `account_data_store_config`.
- [Android app] The Polkadot app for Android: https://github.com/paritytech/polkadot-android-community, checked at commit `e34b2262`.
- [ark-vrf] The `ark-vrf` library, version 0.5.1, function `Secret::from_seed` in `src/lib.rs`: https://crates.io/crates/ark-vrf/0.5.1
- [coinage pallet] The coinage pallet of the runtime: https://github.com/paritytech/individuality-community, directory `pallets/coinage`, checked at commit `7137a05`.
- [durability engine] The durability engine of the TrUAPI core runtime: [TrUAPI core], `rust/crates/truapi/src/durable.rs` and the directory `rust/crates/truapi/src/durable/`.
- [iOS app] The Polkadot app for iOS: https://github.com/paritytech/polkadot-ios-community, checked at commit `b2f32fd7`.
- [schnorrkel] The reference implementation of sr25519: https://github.com/w3f/schnorrkel, files `src/derive.rs` and `src/keys.rs`.
- [sp-core] Substrate's `sp-core` library, the conversion of a path segment into a `DeriveJunction` in `substrate/primitives/core/src/crypto.rs` of https://github.com/paritytech/polkadot-sdk.
- [TrUAPI core] The TrUAPI core runtime: https://github.com/paritytech/host-rust-core
- [verifiable] The ring-proof library: https://github.com/paritytech/verifiable, at commit `1f9f675` in the Android app and `19b03de` in [TrUAPI core]. Both versions compute an entry's secret with [ark-vrf] 0.5.1.
