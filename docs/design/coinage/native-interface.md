---
title: "Coinage native interface"
status: "Draft"
---

# Coinage native interface

The native interface is how a host app's own screens use layer 1 ([`coinage-layer-components.md`](coinage-layer-components.md)): the wallet screens (balance, top-up, offload, recovery) and the chat's payments. It is a thin surface over layer 1's primitives (layer 1, G1) and streams (layer 1, G2), exposed to Swift and Kotlin through UniFFI. Products never use it; they use layer 2 ([`layer-2-components.md`](layer-2-components.md)).

The interface also defines the **hand-over**: the one-time move of a wallet from the coinage engine an app shipped with into the core (section 5). An app turns coinage on in the core, hands its wallet over and stops its own engine in one release, because two engines on one seed compete for the same coins (layer 1, section 6).

In this document, the **app** is the native host app, Android or iOS, that embeds the core. The **runtime** is the core's `NativeTrUApiHostRuntime`.

## 1. What the native interface does not own

| Interface | What it supplies |
|---|---|
| **Layer 1** | Every primitive, stream, type and error (layer 1, G1 and G2). The native interface adds no vocabulary of its own beyond section 3's handle, observers and errors, and section 5's hand-over |
| **The app's chat** | The transport and encryption of payment messages, and the store the messages are kept in. That store is not the core's store (section 4) |
| **The app's screens** | Asking the user to confirm a spend that layer 1 answers with `NeedsConfirmation` (layer 1, A4) |
| **The seed** | The runtime's local signing session (`local_session_secret`). Coinage runs only on the device that holds the seed (layer 1, section 6), so the runtime turns it on only with a local session |

## 2. Turning coinage on

`HostRuntimeConfig` gains one field, `coinage: Option<CoinageConfig>`, default none. When it is given and the runtime has a local session, the runtime calls `enable_coinage` before it starts the core's services, so layer 1 starts with them (layer 1, G3), and passes the hand-over on to layer 1's `start` (section 5). The app reads `registry_contract` from its remote config key `account_data_store_config` (field `contractAddress`), which both shipped wallets already read; when it is none, the stack's contract is used. When `coinage` is none, or the runtime has no local session, coinage stays off and every RFC-6 and RFC-17 call is answered as unsupported, as on a paired host (layer 2, section 1).

```rust
struct CoinageConfig {
    hand_over: Option<Handover>,          // section 5
    registry_contract: Option<String>,    // the AccountDataStore contract, `0x` and 40 hex digits; overrides the stack's
}

// on NativeTrUApiHostRuntime
async fn coinage() -> Result<Arc<NativeCoinage>, CoinageUnavailable>;

enum CoinageUnavailable {
    Off,                                      // `coinage` is none, no local session, or a stack without coinage
    UnsupportedRuntime { check: String },     // layer 1 StartError (G3 invariant 1)
    UndecodableStore,                         // layer 1 StartError (G3 invariant 1)
    NotStarted { reason: String },            // any other StartError
}
```

**`coinage`** waits until layer 1's `start` has returned, then returns a handle on the running layer, or the reason it cannot. It never returns a handle before G3's start sequence has finished, so no call can reach layer 1 before it accepts operations (layer 1, G3 invariant 5). The start itself waits for the seed, which the local session supplies after the core's services have started. Every call returns a handle on the same layer, and after a failed start every call returns the same failure, until the app constructs the runtime again.

## 3. The handle

`NativeCoinage` exposes the primitives the app's screens need, each on the main purse, and with layer 1's own types: the canonical types carry UniFFI derives, and no native mirror of them exists. Every operation the app starts has the key `OpKey { scope: "coinage/app", id }`, with `id` chosen by the app as section 4 describes. Layer 2's scope and every product's scope are distinct from it (layer 2, section 1), so an app key never names a product's operation.

```rust
// ---- Reading ----
async fn balance() -> Result<Balance, Error>;                            // G1 query_purse(MAIN).balance
async fn observe_balance(observer: Arc<dyn BalanceObserver>) -> Result<Arc<NativeSubscription>, Error>;   // G2 balance(MAIN)
async fn observe_holdings(observer: Arc<dyn HoldingsObserver>) -> Result<Arc<NativeSubscription>, Error>; // G2 holdings(MAIN), section 6
async fn recoverable() -> Result<Recoverability, Error>;                 // G1
async fn observe_events(observer: Arc<dyn EventObserver>) -> Result<Arc<NativeSubscription>, Error>;      // G2 events()
fn asset_unit() -> u128;                                                 // G1

// ---- Paying and being paid ----
async fn export(id: [u8; 32], retry: Retry, amount: Amount, confirmed: bool) -> Result<Export, Error>;
async fn commit_export(id: [u8; 32]) -> Result<(), Error>;               // within: none (section 4)
async fn release_export(id: [u8; 32]) -> Result<(), Error>;
async fn reclaim(id: [u8; 32], retry: Retry, export: [u8; 32]) -> Result<Started, Error>;  // into MAIN
async fn import(id: [u8; 32], retry: Retry, coins: Vec<CoinSecret>) -> Result<Started, Error>;  // into MAIN
async fn importing(coins: Vec<CoinSecret>) -> Result<bool, Error>;

// ---- Value in and out ----
async fn top_up(id: [u8; 32], retry: Retry, amount: Amount, funding: Arc<dyn NativeFunding>) -> Result<Started, Error>;
async fn offload(id: [u8; 32], retry: Retry, amount: Amount, to: AccountId, confirmed: bool) -> Result<Started, Error>;

// ---- Following and ending an operation ----
async fn observe_status(id: [u8; 32], observer: Arc<dyn StatusObserver>) -> Result<Arc<NativeSubscription>, Error>;
async fn observe_export_status(id: [u8; 32], observer: Arc<dyn ExportStatusObserver>) -> Result<Arc<NativeSubscription>, Error>;
async fn completion(id: [u8; 32]) -> Result<Option<Completion>, Error>;
async fn cancel(id: [u8; 32]) -> Result<(), Error>;

// ---- Recovery ----
async fn reconstruct(id: [u8; 32], deep: bool) -> Result<Started, Error>;   // Purses::MainOnly
async fn hand_over_progress() -> Result<HandOverProgress, Error>;           // section 5
fn observe_hand_over(observer: Arc<dyn HandOverObserver>) -> Arc<NativeSubscription>;

// ---- Observers, implemented by the app ----
trait BalanceObserver      { fn on_balance(balance: Balance); }
trait HoldingsObserver     { fn on_holdings(holdings: Holdings); }
trait StatusObserver       { fn on_status(item: StatusItem); }
trait ExportStatusObserver { fn on_export_status(coins: Vec<ExportedCoinStatus>); }
trait EventObserver        { fn on_event(event: Event); }
trait HandOverObserver     { fn on_progress(progress: HandOverProgress); }
trait NativeFunding        { fn account() -> AccountId; fn sign(payload: Vec<u8>) -> Option<Signature>; }  // G1 Funding
```

- `Error` is layer 1's (G1), crossing as a flat error named `CoinageError`: its variant and message. Each call does what its layer-1 primitive does, with these arguments fixed: the purse is the main purse, and `export` and `offload` pass `Initiator::User`, because the app's screens act only on the user's instruction. `reclaim`'s `export` argument is the `id` the export was started under.
- Each `observe_*` call delivers what its G2 stream emits, in order, to the observer; `observe_hand_over` delivers the current `HandOverProgress` at once and then each change, as G2's value streams do, until the app cancels the returned subscription, or the stream closes (layer 1, G2). A subscription does not survive a restart of the runtime; the app subscribes again (layer 1, G2 invariant 5).
- A `NeedsConfirmation` answer means the spend needs value the user has not agreed to use (layer 1, A4). The app asks the user, and on agreement calls again with the same `id` and `confirmed: true`.
- The core follows the People chain's finalized heads and runs layer 1's maintenance on each one while the runtime runs (layer 1, F2 invariant 2). The app keeps the runtime running in the background as it does for the durability engine's open work.

## 4. Chat payments

The chat carries a payment as a message that holds the coins' secrets. Its messages are in the app's store, so the app cannot pass a store transaction to `commit_export`, and it follows layer 1's ordering rule instead: it commits the export **before** it makes the secrets durable anywhere (layer 1, G1 invariant 3). The other order is the only one that loses money.

**Paying.** For each payment, the app:

1. Draws a random `id` and saves it in its own store, with the recipient and the amount, before step 2. This is what lets it find the payment again after a crash.
2. Calls `export(id, retry, amount, confirmed)`. On `NeedsConfirmation` it asks the user (section 3). On success it holds the coins and their secrets.
3. If the user abandons the payment now, calls `release_export(id)`, and stops.
4. Calls `commit_export(id)`.
5. Saves the payment message, with the secrets and `id`, and sends it.
6. Follows the payment with `observe_export_status(id)`, which reports each coin as not yet minted, awaiting the payee, claimed, reclaimed or never minted (layer 1, B1).

After a restart, the app looks at each `id` saved in step 1 that has no message from step 5. If the export was never committed, layer 1 has already released it at start (layer 1, G3 invariant 5), and `completion(id)` reports `Failed(NotCommitted)`. If it was committed, nobody received the secrets, and the app calls `reclaim` for it. The user can also ask to reclaim a payment that was sent but not claimed; whichever side moves a coin first owns it (layer 1, C1 invariant 4). A reclaim's `id` is `BLAKE2b-256("coinage/chat-reclaim" ‖ export id ‖ n)`, where `n` is the number of earlier reclaims of that export that have ended, SCALE-encoded as `u32`, so a repeated request joins the reclaim that is running and a later one starts a new reclaim (layer 2, K4 invariant 3).

**Being paid.** For each payment message received, the app calls `import(id, retry, coins)` with `id = BLAKE2b-256("coinage/chat-claim" ‖ message id)` and `retry = Until(the message's timestamp + the claim window)`, and follows it with `observe_status(id)`. The key is derived from the message, so a second call for the same message, after a restart or from a second delivery, joins the first claim instead of starting another (layer 1, B4). `Completion::coins` reports, per coin, whether it was claimed, taken by someone else, never seen, or left unclaimed (layer 1, G1).

**Secrets in the app.** The app holds a coin secret only while it handles it: an export's secrets go into the payment message, and a received message's secrets go to `import`. Neither is logged.

## 5. Hand-over from a shipped wallet

The hand-over carries what the app knows and the chain does not: how far each of its installations allocated, which payments it sent and are not settled, and which payments it received and has not claimed. With these, the core finds every asset the app held without relying on layer 1's gap rule, and without the two residual risks layer 1 accepts after a wipe, lost high-water marks and lost disclosure marks (layer 1, F4).

**Terms.**

- **High-water mark:** for one installation and one index space (coins or entries), the lowest index the app never allocated. Every index the app allocated in that space is below it. The app allocated only in the main purse.
- **Outstanding payment:** a payment whose coin secrets the app made durable in a message, and for which the app has not observed that every coin was claimed or reclaimed.
- **Unclaimed incoming payment:** a payment message the app received whose claim it has not seen finalized.

```rust
struct Handover {
    installations: Vec<AppInstallation>,
    outstanding: Vec<OutstandingPayment>,
    incoming: Vec<IncomingPayment>,
}
struct AppInstallation { id: InstallationId, next_coin: u32, next_entry: u32 }        // the two high-water marks
struct OutstandingPayment { id: [u8; 32], accounts: Vec<AccountId> }               // the coin accounts of the secrets in the sent message
struct IncomingPayment { id: [u8; 32], claim_by: Time, coins: Vec<CoinSecret> }      // id as in section 4
enum HandOverProgress { None, Scanning, Done }   // None: no hand-over was recorded
```

**Sequence.**

1. The app stops its engine: from this step on, it starts no operation, and submits and rebuilds no transaction. Its operations in flight stop where they are and are not handed over. Transactions it already submitted land or expire, and the scan finds the value they leave, so nothing is lost; but an operation of several steps, such as an offload to a merchant between its phases, does not complete.
2. The app builds the `Handover` from its own state and constructs the runtime with `coinage: Some(CoinageConfig { hand_over: Some(handover) })`, then calls `coinage()`.
3. Once `coinage()` returns the handle, the app deletes its own coinage state. Until then it keeps it: if the app stops before, it repeats step 2 at its next start, with a `Handover` built from its state at that time.

**Recording.** The hand-over is recorded only when layer 1's store names no current installation, in the store transaction of G3's step 2 that mints one (layer 1, G3 invariant 3). So it is recorded before the start sequence runs and before any reconstruction can run, and it is recorded at most once: on any later start an installation exists, and a `Handover` in the config is ignored. The app therefore turns coinage on for the first time together with its hand-over. In that transaction, the core:

- adds every installation in `installations` to B2 (`add_installations`), with its two high-water marks;
- records each outstanding payment as a committed export under the key `OpKey { scope: "coinage/app", id }`, with its accounts and no coins yet;
- records each unclaimed incoming payment's `id`, `claim_by` and secrets, which B2 keeps until the payment's import has been started.


**Imports.** At every start, after the start sequence, the core starts or joins `import` for each recorded incoming payment, under `OpKey { scope: "coinage/app", id }` with `retry = Until(claim_by)`, and deletes the payment's secrets from the hand-over record once the import is started (B4 holds them from then on, layer 1, G1 invariant 5). A payment whose import layer 1 refuses, for example because a secret is not a canonical key (`InvalidSecret`), is deleted from the record and logged; nobody can claim such a coin. A coin that already left its account, through a claim the app submitted before step 1 or through the payer's reclaim, reports `TakenByOther`; a coin the app's own claim moved is found by the scan.

**Scanning.** An installation with stored high-water marks is scanned by layer 1's reconstruction (F4) with one change: in each index space, the scan does not stop before the space's high-water mark, and it applies the gap rule only past it. The default reconstruction scans it, as it scans every installation whose first scan is not done, no earlier than one longest validity window after the installation was minted (layer 1, F4 invariant 5). Since the hand-over is recorded in the same transaction as the mint, every transaction the app submitted before step 1 has landed or expired by then.

When a scan adopts a coin whose account an outstanding payment lists, it adopts the coin with a committed disclosure mark (B2), as a coin of that payment's export, in the same store transaction. So no coin of an outstanding payment is ever selected, `observe_export_status(id)` reports it, and `reclaim` takes it back. A listed account the scan does not find held no coin when it was read: the payee had claimed it, or it was never minted. Such a coin is not part of the export, so the export status of a handed-over payment covers only the coins the scan adopted, and is complete once `hand_over_progress()` is `Done`.

`hand_over_progress()` is `None` when no hand-over was recorded, `Scanning` while a handed-over installation's first scan is not done, and `Done` once every one is done. Until it is `Done`, the balance does not include all of the app's wallet, and the app can say the balance is still being moved.
## 6. Changes to layer 1

The native interface needs these changes in layer 1 ([`coinage-layer-components.md`](coinage-layer-components.md)).

- **Building waits for the registration (G3 invariant 4).** With a registry contract configured, the layer builds no transaction until its installation's registration is finalized, so no value reaches the chain under an installation the registry does not list, as in both shipped wallets. Operations still start and their records wait to be built (C2 invariant 5). After the hand-over the core is the only engine on the seed, and a wipe before the registration landed would otherwise lose what the core placed on chain. Without a registry contract, nothing waits, as for NFT purses: the wallet runs, and `recoverable()` stays `Pending` so the app can say it cannot be recovered from the seed alone.
- **The hand-over (section 5).** G3's step 2 records it; B2 keeps each handed-over installation's high-water marks, each outstanding payment's accounts and each recorded incoming payment; and F4's scan does not stop before a high-water mark, and adopts a listed account's coin as a coin of its payment's export.
- **Holdings (G2).** A fifth stream, `holdings(purse)`, emits the purse's `Holdings` at once, then each change, under the same rules as `balance` (G2 invariant 1). The balance and the rows are computed from one snapshot, so a screen that shows both never shows them disagreeing.

```rust
struct Holdings { balance: Balance, rows: Vec<Holding> }
struct Holding {
    asset: Asset,                 // a coin or an entry of the purse (B2), which identifies the row across changes
    exponent: Exponent,
    ready: bool,                  // selectable now (B1 Selectability::Now); otherwise counted as held back or arriving
    age: Option<u32>,             // a coin's age as the chain reports it (F1); none for an entry
    batch_unloaded: bool,         // a coin this layer created by an unload, which the chain shows at age one, as it does for one of a batch
    fungibility: Option<u8>,      // percent, see below
}
```

`fungibility` is how much of a ring's anonymity a row hides in, as both shipped wallets compute it: `round(100 · (I² − U²) / (I · L))`, where `L` is the ring's capacity, `I` the members its root includes, and `U` the aliases unloaded in it, clamped to `I`; zero when `I` or `L` is zero. For an entry it is its ring's value as last observed, and none while the entry is not in a ring. For a coin this layer created by an unload, it is the value of the ring the unload left, as the unload's input entry was last observed in it. For every other coin it is none. So F1 also reads a ring's unloaded count and stores it with the entry's observation; on a runtime that keeps no such count the value is none.

## 7. Open decisions

None.
