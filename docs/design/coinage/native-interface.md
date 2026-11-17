---
title: "Coinage native interface"
status: "Draft"
---

# Coinage native interface

The native interface is how a host app's own screens use layer 1 ([`coinage-layer-components.md`](coinage-layer-components.md)): the wallet screens (balance, top-up, offload, recovery) and the chat's payments. It is a thin surface over layer 1's primitives (layer 1, G1) and streams (layer 1, G2), exposed to Swift and Kotlin through UniFFI. Products never use it; they use layer 2 ([`layer-2-components.md`](layer-2-components.md)).

It has three components: turning coinage on (V1), the handle the app's screens call (V2), and the hand-over of the wallet the app's own coinage engine held (V3). Layer 1 owns what happens to a hand-over once the app has given it: B2 stores it, G3 records it and claims its incoming payments, and F4 scans its installations.

In this document, the **app** is the native host app, Android or iOS, that embeds the core. The **runtime** is the core's `NativeTrUApiHostRuntime`.

## 1. What the native interface does not own

| Interface | What it supplies |
|---|---|
| **Layer 1** | Every primitive, stream, type and error (layer 1, G1 and G2), the hand-over's types (G1), and what happens to a hand-over (B2, F4, G3 invariant 8). The native interface adds no vocabulary of its own beyond V1's configuration and errors and V2's handle and observers |
| **The app's chat** | The transport and encryption of payment messages, and the store the messages are kept in. That store is not the core's store (V2 invariant 2) |
| **The app's screens** | Asking the user to confirm a spend that layer 1 answers with `NeedsConfirmation` (layer 1, A4), and keeping the user's privacy setting |
| **The seed** | The runtime's local signing session (`local_session_secret`). Coinage runs only on the device that holds the seed (layer 1, section 6), so the runtime turns it on only with a local session |

## 2. Component catalogue

### Band V — Native surface
*Thin: configuration, one handle and one input. Nothing here is durable.*

---

#### V1 · Turning coinage on

**Owns** nothing. **Band:** thin surface.

```rust
// on HostRuntimeConfig
coinage: Option<CoinageConfig>,           // default none

struct CoinageConfig {
    hand_over: Option<Handover>,          // V3; the type is layer 1's (G1)
    registry_contract: Option<String>,    // the AccountDataStore contract, `0x` and 40 hex digits; overrides the stack's
}

// on NativeTrUApiHostRuntime
async fn coinage() -> Result<Arc<NativeCoinage>, CoinageUnavailable>;

enum CoinageUnavailable {
    Off,                                      // `coinage` is none, no local session, or a stack without coinage
    UnsupportedRuntime { check: String },     // layer 1 StartError::UnsupportedRuntime (G3 invariant 1)
    UndecodableStore,                         // layer 1 StartError::UndecodableStore (G3 invariant 1)
    NotStarted { reason: String },            // any other StartError, with its message
}
```

- **Construction.** When `coinage` is given and the runtime has a local session, the runtime turns coinage on before it starts the core's services, so layer 1 and layer 2 start with them (layer 1, G3), with the hand-over and the registry contract as the configuration gives them. A `registry_contract` that is not 20 bytes in hex makes the runtime's construction fail (`NativeRuntimeConfigError::Invalid`). When `coinage` is none, or the runtime has no local session, coinage stays off, and the layers serve no RFC-6 or RFC-17 call (layer 2, section 1).
- **The registry contract.** The app reads it from its remote config key `account_data_store_config` (field `contractAddress`), which both shipped wallets read. The app's contract takes precedence over the one the stack names. With neither, the wallet runs without a registry, cannot be recovered from the seed alone, and `recoverable()` stays `Pending` (layer 1, G3 invariant 4).
- **`coinage`** waits until layer 1's `start` has returned, then returns a handle on the running layer, or the reason it cannot. The start itself waits for the seed, which the local session supplies after the core's services have started. Every call returns a handle on the same layer, and after a failed start every call returns the same failure, until the app constructs the runtime again.
- **Driving the layers.** Once layer 1 has started, the core follows the People chain's finalized heads for as long as the runtime lives. For each head it calls layer 1's `on_block` (F1) and then layer 2's `run_due` (layer 2, section 1), so the app does not call the entry point while the runtime runs. When the node's subscription ends, the core subscribes again after six seconds.

**Invariants**
1. **No call reaches layer 1 before it accepts operations.** `coinage` never returns a handle before G3's start sequence has finished (layer 1, G3 invariant 5).
2. **Coinage runs only where the seed is.** Without a local session there is no layer, and `coinage` answers `Off`.

---

#### V2 · The handle

**Owns** nothing. **Band:** thin surface.

`NativeCoinage` exposes the primitives the app's screens need, each on the main purse, and with layer 1's own types: the canonical types carry UniFFI derives, and no native mirror of them exists. Every operation the app starts has the key `OpKey { scope: "coinage/app", id }`, with `id` chosen by the app (invariant 1). Layer 2's scope and every product's scope are distinct from it (layer 2, section 1), so an app key never names a product's operation.

```rust
// ---- Reading ----
async fn balance() -> Result<Balance, Error>;                            // G1 query_purse(MAIN).balance
async fn observe_balance(observer: NativeBalanceObserver) -> Result<NativeSubscription, Error>;    // G2 balance(MAIN)
async fn observe_holdings(observer: NativeHoldingsObserver) -> Result<NativeSubscription, Error>;  // G2 holdings(MAIN)
async fn recoverable() -> Result<Recoverability, Error>;                 // G1
async fn observe_events(observer: NativeEventObserver) -> Result<NativeSubscription, Error>;       // G2 events()
fn asset_unit() -> u128;                                                 // G1

// ---- Paying and being paid ----
async fn export(id: [u8; 32], retry: Retry, amount: Amount, confirmed: bool) -> Result<Export, Error>;
async fn commit_export(id: [u8; 32]) -> Result<(), Error>;               // within: none (invariant 2)
async fn release_export(id: [u8; 32]) -> Result<(), Error>;
async fn reclaim(id: [u8; 32], retry: Retry, export: [u8; 32]) -> Result<Started, Error>;  // into MAIN
async fn import(id: [u8; 32], retry: Retry, coins: Vec<CoinSecret>) -> Result<Started, Error>;  // into MAIN
async fn importing(coins: Vec<CoinSecret>) -> Result<bool, Error>;

// ---- Value in and out ----
async fn top_up(id: [u8; 32], retry: Retry, amount: Amount, funding: NativeFunding) -> Result<Started, Error>;
async fn offload(id: [u8; 32], retry: Retry, amount: Amount, to: AccountId, confirmed: bool) -> Result<Started, Error>;

// ---- Following and ending an operation ----
async fn observe_status(id: [u8; 32], observer: NativeStatusObserver) -> Result<NativeSubscription, Error>;
async fn observe_export_status(id: [u8; 32], observer: NativeExportStatusObserver) -> Result<NativeSubscription, Error>;
async fn completion(id: [u8; 32]) -> Result<Option<Completion>, Error>;
async fn cancel(id: [u8; 32]) -> Result<(), Error>;

// ---- Recovery and privacy ----
async fn reconstruct(id: [u8; 32], deep: bool) -> Result<Started, Error>;   // Purses::MainOnly, Retry::Never
async fn set_privacy(preset: PrivacyPreset) -> Result<(), Error>;            // A4
async fn hand_over_progress() -> Result<HandOverProgress, Error>;           // G1
fn observe_hand_over(observer: NativeHandOverObserver) -> NativeSubscription;   // G2 hand_over()

// ---- Implemented by the app ----
trait NativeBalanceObserver      { fn on_balance(balance: Balance); }
trait NativeHoldingsObserver     { fn on_holdings(holdings: Holdings); }
trait NativeStatusObserver       { fn on_status(item: StatusItem); }
trait NativeExportStatusObserver { fn on_export_status(coins: Vec<ExportedCoinStatus>); }
trait NativeEventObserver        { fn on_event(event: Event); }
trait NativeHandOverObserver     { fn on_progress(progress: HandOverProgress); }
trait NativeFunding              { fn account() -> AccountId; fn sign(payload: Vec<u8>) -> Option<Signature>; }  // G1 Funding
```

- `Error` is layer 1's (G1), crossing UniFFI as a flat error named `CoinageError`: its variant and message. Each call does what its layer-1 primitive does, with these arguments fixed: the purse is the main purse, and `export` and `offload` pass `Initiator::User`, because the app's screens act only on the user's instruction. `reclaim`'s `export` argument is the `id` the export was started under.
- Each `observe_*` call delivers what its G2 stream emits, in order, to the observer, until the app cancels the returned subscription or the stream closes (layer 1, G2). A subscription does not survive a restart of the runtime; the app subscribes again (layer 1, G2 invariant 5).
- A `NeedsConfirmation` answer means the spend needs value the user has not agreed to use (layer 1, A4). The app asks the user, and on agreement calls again with the same `id` and `confirmed: true`.
- **`set_privacy`** stores the policy of the preset the user chose (layer 1, A4 `preset_policy`, G1 `set_policy`); every preset passes A4's check. Layer 1 keeps the stored policy across restarts, and the core starts a fresh store with *Fastest* (layer 1, A4). The app keeps the user's setting itself, and calls `set_privacy` whenever the user changes it and after each start, so a fresh store gets the user's setting too.

**Invariants.** These are the rules an app follows; the core relies on them and cannot check them.
1. **An id names one operation.** A call whose `id` names an operation with the same arguments joins it, and one with other arguments is refused with `KeyReused` (layer 1, B4). So the app derives each id from something it has made durable before the call, such as the id of the chat message a payment travels in, and the same call after a crash or a second delivery joins the first instead of starting another. A new attempt that must not join an ended one, such as a second reclaim of one export after the first ended, needs a new id.
2. **An export is committed before its secrets become durable anywhere.** The app keeps its messages in its own store, so it cannot pass a store transaction to `commit_export`, and it follows layer 1's ordering rule instead: commit no later than the moment it makes the secrets durable anywhere else (layer 1, G1 invariant 3). The other order is the only one that loses money. A payment the user abandons before the commit is released with `release_export`.
3. **After a crash, every export ends.** An export that was never committed is released at the next start (layer 1, G3 invariant 5), and `completion` reports `Failed(NotCommitted)`. A committed export whose secrets reached nobody is taken back with `reclaim`. The user can also reclaim a payment that was sent but not claimed; whichever side moves a coin first owns it (layer 1, C1 invariant 4).
4. **A payment received is claimed under an id derived from the payment**, with `Retry::Until` its claim deadline, so a second delivery of the same message joins the first claim. An incoming payment the app hands over (V3) carries the id the app claims it under, so a claim the app starts later joins the import the core started.
5. **The app holds a coin secret only while it handles it**: an export's secrets go into the payment message, and a received message's secrets go to `import`. Neither is logged.
6. **The app keeps the runtime running in the background** as it does for the durability engine's open work, so the layers keep observing the chain and running what is due (layer 1, F2 invariant 2).

---

#### V3 · The hand-over

**Owns** nothing: layer 1's B2 stores the hand-over once the app has given it. **Band:** thin surface.

The hand-over moves the wallet of the coinage engine an app shipped with into the core, once. It carries what the app knows and the chain does not, in layer 1's `Handover` (G1): every installation the app allocated under, with its two high-water marks; every **outstanding payment**, one whose coin secrets the app handed to someone (in a chat message or to a merchant) and whose coins it has not seen claimed or reclaimed, with the coin accounts of those secrets; and every **incoming payment**, a payment message the app received and whose claim it has not seen finalized, with the secrets it carries. A **high-water mark**, for one installation and one index space (coins or entries), is the lowest index the app never allocated in the main purse; the app allocated only in the main purse.

The app moves its wallet in this order:

1. The app stops its engine: from this step on, it starts no operation, and submits and rebuilds no transaction. Its operations in flight stop where they are and are not handed over (layer 1, section 5).
2. The app builds the `Handover` from its own state, constructs the runtime with `coinage: Some(CoinageConfig { hand_over: Some(handover), .. })` (V1), and calls `coinage()`.
3. Once `coinage()` returns the handle, the app deletes its own coinage state. Until then it keeps it: if the app stops before, it repeats step 2 at its next start, with a `Handover` built from its state at that time.

What the core does with it is layer 1's: B2 records it in the store transaction that mints the installation, so only on a fresh store (G3 step 2); G3 claims its incoming payments at every start (G3 invariant 8); F4 scans its installations up to their high-water marks and adopts the coins of its outstanding payments as coins of their exports; and `hand_over_progress()` reports `None` when no hand-over was recorded, `Scanning` while a handed-over installation's first scan is not done, and `Done` once every one is done. Until it is `Done`, the balance does not include all of the app's wallet, and the app can say the balance is still being moved.

**Invariants**
1. **The first start carries the hand-over.** The core records a hand-over only on a fresh store and ignores one on any later start (layer 1, G3 invariant 8). So an app turns coinage on for the first time together with its hand-over.
2. **A hand-over the app cannot build leaves coinage off.** When the app cannot read its engine's state, it does not turn coinage on for that run, because a start without the hand-over would make it impossible to record later.

## 3. Open decisions

None.
