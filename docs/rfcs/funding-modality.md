---
title: "Funding modality: the provider worker"
owner: "@filvecchiato"
status: draft
---

# RFC — Funding modality: the provider worker

## Summary

A funding provider moves value into or out of the user's balance. Its Worker runs each session end to end: it takes the
user's payment on its own rail, converts it, and credits the user with `payment.topUp`, or collects with
`payment.request` and pays out. The core assigns sessions, asks every provider for quotes, stores what the worker
reports, and decides the outcome itself from the top-ups and payments the worker names.

This RFC covers the worker's side, the `FundingProvider` trait. A product opens a session with `funding.request` and
the host draws the overlay; those are defined with the `Funding` trait and the host's `FundingPlatform`. The Worker
manifest's `includes.funding` is defined in the [product manifest RFC](product-manifest.md#funding-configuration).

## Motivation

Onramps differ in every step that matters: the rails they take, their KYC, how they convert and pay out, and the
countries they serve. Hard-coding each one in the core, or in every host, does not scale and has to be redone per
platform. A provider's own worker can follow its service's steps, while the core keeps what must not depend on a
provider: who is asked, which session a provider may report on, and whether value actually moved.

## Approach

```mermaid
flowchart LR
  manifests["Manifests<br/>host list · funding"]
  product["Product<br/>funding.request()"]
  subgraph host["Host"]
    overlay["Overlay UI<br/>list · summary · frame"]
    pocket["Pocket<br/>balance card · history"]
  end
  subgraph core["Rust core · funding modality"]
    discovery["Discovery + quotes"]
    sessions["Sessions + report store<br/>validates, persists, maps status"]
    payments["Payment API<br/>topUp · request"]
  end
  subgraph worker["Provider worker"]
    provider["Provider worker<br/>answerQuote · serve · report"]
    ephemeral["Ephemeral accounts<br/>deriveEntropy"]
  end
  onramp["Onramp provider<br/>KYC · card · bank"]
  chain["Asset Hub / People<br/>→ CASH, teleport"]

  manifests --> discovery
  product --> sessions
  overlay <-->|quotes · select| discovery
  sessions -->|balance · history| pocket
  discovery -->|quote| provider
  sessions -->|Assigned / Cancel| provider
  provider -->|report| sessions
  provider -->|payment.topUp| payments
  provider -->|signs in · quotes · orders| onramp
  provider -->|keeps its auth token| host
  provider --- ephemeral
  ephemeral --> chain
```

A provider signs in to its own API itself and keeps its tokens with the host; the Worker manifest names no backend.
The worker prices quotes and runs sessions against that API.

## The `FundingProvider` trait

Products keep `Funding`; the worker side is a trait of its own because execution gating applies to a whole trait.

```rust
#[wire_trait(id = 24)]
#[service(required_execution = Worker)]
pub trait FundingProvider {
    #[wire(id = 0)] // sessions in flight first, then new items
    async fn serve_subscribe(&self, cx) -> Subscription<HostFundingServeSubscribeItem, _>;
    #[wire(id = 1)] // stored before it answers
    async fn report(&self, cx, request: HostFundingReportRequest) -> Result<(), HostFundingReportError>;
    #[wire(id = 2)] // an App route or an external https page, answers once the frame closes
    async fn present_frame(&self, cx, request: HostFundingPresentFrameRequest)
        -> Result<HostFundingPresentFrameResponse, HostFundingPresentFrameError>;
    #[wire(id = 3)]
    async fn answer_quote(&self, cx, request: HostFundingAnswerQuoteRequest) -> Result<(), HostFundingAnswerQuoteError>;
    #[wire(id = 4)] // at most 4 KiB
    async fn save(&self, cx, request: HostFundingSaveRequest) -> Result<(), HostFundingSaveError>;
}
```

### `serve_subscribe`

```rust
enum HostFundingServeSubscribeItem {
    Assigned { session: FundingAssignment },  // chosen for this provider, or still in flight on open
    Cancel { intent: String },                // the user asked to cancel
    CancelAccepted { intent: String },        // the user accepted the provider's CancelOffer
    CancelDeclined { intent: String },        // the user declined it, or it lapsed
    Quote { ask_id: String, ask: FundingQuoteAsk },
}

struct FundingAssignment {
    intent: String,
    direction: FundingDirection,             // In | Out
    amount: Option<u128>,                    // None: the user picks it on the provider's screens
    expires_at: Option<u64>,                 // Unix ms; None once funds move
    last_update: Option<FundingUpdate>,      // where a restarted worker resumes
    quote: Option<FundingQuote>,             // the quote the user chose
    saved: Option<Vec<u8>>,                  // what `save` last stored
}
```

The stream replays every session still in flight and every unanswered ask when it opens, so a restarted worker carries
on without its own bookkeeping, including a cancel it has not answered and the user's answer to its offer. Before money
moved, a worker answers `Cancel` by reporting `Failed { Cancelled }` if it can still stop. After, it answers with
`CancelOffer` or `CancelRefused` (see [Cancel](#cancel)).

### `answer_quote`

The core sends every ask to every funding provider, whatever its manifest declares. The worker prices it from its own
API and answers by `ask_id`.

```rust
struct FundingQuoteAsk {
    direction: FundingDirection,
    rail: FundingRail,                       // Card | Bank | Crypto
    asset: String,                           // symbol paid (In) or received (Out)
    network: Option<String>,                 // Crypto only, such as "polkadot"
    amount: u128,                            // balance units, credited (In) or debited (Out)
    country: Option<String>,                 // ISO 3166-1 alpha-2: the user's pick, else the host's guess
}

enum FundingQuoteAnswer {
    Quoted { quote: FundingQuote },
    Refused { reason: FundingQuoteRefusal }, // CountryUnsupported | BelowMinimum { min }
                                             // | AboveMaximum { max } | Unavailable | Other { message }
}

struct FundingQuote {
    quote_id: String,
    send_amount: u128,                       // asset units (In), balance units (Out)
    receive_amount: u128,                    // balance units (In), asset units (Out)
    provider_fee: u128,                      // in the unit of send_amount
    network_fee: u128,
    eta_secs: Option<u64>,
    expires_at: Option<u64>,
    asset_decimals: u8,                      // decimals of the ask's asset, so the host shows its amounts
}
```

An ask unanswered after 10 seconds shows as timed out. An answer is reused for 30 seconds. The host's list marks the
best quote among providers a session can be handed to, the least to pay inbound or the most to receive outbound, and
the host selects it until the user picks another. While the user has not chosen, the core asks every provider again
each minute. Each request for quotes has an id that every list it produces carries. A list stops at once, mid round
too, when the session is handed to a provider or ends, the host asks again, or the host stops it, so a late answer
for an amount the user already changed never reaches the host.

Until the user picks a country, an ask carries the one the host detects without asking for a permission: the device's
coarse location if already permitted, then its SIM or network country, then its locale's region. Only an ISO 3166-1
alpha-2 code counts; a detection is reused for an hour, and one that found none is tried again after a minute. What
answers show about a provider, served or refused for a country the ask named, is kept for 12 hours and folded into
its manifest's routes while that manifest serves Funding. A minimum or maximum a refusal showed is kept by provider,
rail, asset, network and country, for 12 hours from that refusal: a later quote does not renew it, and a quote at an
amount the limit rules out clears it. The core combines them into the range a session can take across every
provider, the smallest minimum and the largest maximum, with no bound while any provider has not shown one, and
never below $10 CASH. Quotes are asked of, and a session can be handed to, exactly the providers whose
manifest serves Funding, whatever routes it declares: one still waiting for its manifest, or whose manifest is withdrawn
or stops serving Funding, is neither asked nor chosen.

The user picks a provider by one of its quotes for the session. When the session has an amount, the quote must price
it: `receive_amount` inbound, `send_amount` outbound, both in balance units. A session opened without an amount takes
the quote's.

### `report`

```rust
enum FundingUpdate {
    AwaitingPayment,                                  // In
    PaymentReceived { finalized: bool, mismatch: Option<FundingReceived> }, // In
    Converting,                                       // In; Out: once, after release, before Payout
    Crediting { top_up_id: [u8; 32], amount: u128 },  // In, up to 3
    Delivered,                                        // In: no more top-ups
    Collecting { payment_id: [u8; 32], amount: u128 },// Out
    Failed { reason: FundingFailure },                // before funds move
    Deposit { deposit: FundingDeposit },              // In, until funds move, outside the order
    Details { transaction_id: Option<String>, reference: Option<String> }, // any time, outside the order
    Payout { outcome: FundingPayout },                // Out: once, after release
    Refunding { top_up_id: [u8; 32], amount: u128 }, // Out: once, after a failed payout
    PayoutRetrying,                                   // Out: after release, before Payout, at each retry
    CancelRefused { reason: String },                 // answers a Cancel, outside the order
    CancelOffer { fee: u128, refund: u128, expires_at: Option<u64> }, // answers a Cancel, outside the order
}

enum FundingDeposit {
    Crypto { address, network, asset, amount, decimals, exact: bool, uri: Option<String>, expires_at: Option<u64> },
    Bank { amount, currency, decimals, beneficiary, account, bank_code, reference, expires_at },
}
```

- Only the provider a session is assigned to may report on it, and updates only move forward. Otherwise the report
  fails with `NotFound` or `OutOfOrder`.
- The core stores each report before answering and keeps the session until the host has written it to its history. After that it keeps the newest 64 recorded sessions per product in full and compacts older ones into a session pack it never trims (intent, owner, direction, amounts, outcome and times), so a product watching any session it opened gets its terminal item instead of `NotFound`, and the host can list the pack as history.
- The core records every top-up and payment request a provider's worker starts while it serves an open session in
  that direction, with the amount: on the one such session asking for that amount when exactly one does, otherwise
  on each of them (the report names which). A withdrawal whose payout failed takes top-ups too, as its refund,
  asking for what it debited. `Crediting` and `Refunding` must name a recorded top-up and `Collecting` a recorded
  payment request, at the recorded amount, once across the provider's sessions; anything else fails with
  `NotStarted`. Funds started before the session was assigned were never recorded for it.
- `Converting`, `Payout`, `Refunding` or `PayoutRetrying` on a withdrawal that is not released yet fails with
  `OutOfOrder`, and so does `PayoutRetrying` once the payout outcome is reported.
- A report on a session the user chose a provider for while the overlay is still up is stored like any other.
- `Deposit` and `Details` carry what the host draws: where and what the user pays, and the provider's references. The
  latest of each is shown.

### `present_frame`

The host shows provider screens in a frame it owns: an iframe in a browser host, a WebView in a native one.

```rust
struct HostFundingPresentFrameRequest { intent: String, target: FundingFrameTarget }

enum FundingFrameTarget {
    App { route: String },                              // a route of the provider's own App
    Page { url: String, return_url: Option<String> },   // an external https page, such as an onramp's widget
}

enum FundingFrameOutcome {
    Closed,                                             // the screen finished and closed itself
    Dismissed,                                          // the user closed it
    Returned { url: String },                           // the page reached `return_url`; the full URL, query included
}
```

- A page and its return URL must be https URLs with a host and no credentials; anything else fails with `InvalidUrl`
  before the host sees it. Any https origin is allowed, since onramps hand users on to changing sub-providers.
- The host shows the page's origin in its own chrome, with a close button, and allows what KYC and card checks need:
  scripts, forms, popups and the camera.
- When the page navigates to `return_url`, the host closes the frame and hands the URL back, so the worker reads the
  result from its query. This works the same in an iframe and a native WebView, where a page has no parent to post to.

### `save`

`save` stores opaque state for a session, such as the provider's order id, and replaces what was saved before. It
comes back in `Assigned.saved` and is dropped once the provider has no more work on the session: for a withdrawal,
after the payout and any refund.

## Outcome

- **The core decides success, never the worker.** An inbound session is `Delivered { credited }` with the recorded
  amounts of its named top-ups whose claims are final (`Claimed { finalized: true }`), what a partial claim reports, or
  nothing for one not claimed. An outbound session is `Released { debited }` with the recorded amount once its named
  payment completes. Nothing moved ends the session as failed.
- **Undecided statuses are read again.** A claim that is not final, a status stream that ends or fails first, or a
  top-up or payment the host does not know yet leaves the session open, and the core reads the status again with
  backoff. One the host still does not know an hour after the report named it, the time a payment can wait on the
  user's approval, counts as nothing moved.
- **A failed payout is refunded.** The user's CASH left with the release, so a provider whose payout failed gives it
  back with a top-up it names in `Refunding`. The core follows its claims and keeps what they returned beside the
  `Released` outcome. The provider keeps the session until it has reported the payout and any refund, whether or not
  the host recorded it, and nothing expires it meanwhile. A provider that never reports its payout therefore holds the
  session, and its worker, for good.
- **Money moving locks the session.** Money moved once the provider started a top-up or payment request for the
  session, named or not, or reported `PaymentReceived`. From then on only the provider ends the session: it cannot
  fail it once funds started, no cancel ends it and nothing expires it. A worker can stop between starting funds and
  naming them, so a recorded start locks the session as a named one does.
- **Expiry** ends a session as `Failed { Expired }`: 24 hours after it opened while no provider has taken it, and 7
  days after the provider's last update while it is assigned and no money moved, so a provider that went silent cannot
  hold it. A bank transfer can take days, so a provider that keeps reporting keeps the session.
- **Closing the overlay after choosing a provider** keeps the session as started, since the provider may already be
  working on it, and asks the provider to stop unless money moved.
- **One status per session.** The host words a typed status: waiting for the transfer, payment or deposit by rail,
  converting, initiated, converting to the asset or preparing the payout, retrying the top-up or the payout, taking
  longer once the provider has been silent past the chosen quote's `eta_secs`, the user's cancel while it is open
  (requested, offered, refused, cancelling), and how it ended, including cancelled with what the refund returned. A
  top-up waits on the user until the provider holds the payment, so it keeps the waiting status however long that
  takes; a withdrawal's clock runs from the quote.
- **Ids are scoped.** The core hashes the provider's product into every top-up and payment id it hands the host, and
  follows them under the same id.

## Resuming after a restart

The host app can be closed or crash at any point; a session carries on from its last checkpoint when it starts again.

- **What the core keeps.** Every session, every report (stored before the report is answered), what the provider last
  `save`d, and the top-ups and payment requests recorded for it, by id. On start the core reloads them, takes the
  worker of every session a provider still serves, and follows each named top-up and payment again.
- **What the provider does.** It calls `save` before every external action: a request to its onramp, a faucet, a
  `payment.topUp` or `payment.request`. It derives the ids of its top-ups, payment requests and onramp orders from
  the session and the step, so repeating one after a crash finds it (`AlreadyExists`) instead of moving funds twice.
  On `Assigned` it resumes from `last_update` and `saved`, not from the start.
- **What the host does.** It starts funding at launch when it has sessions in flight, rather than waiting for a
  product to need the core, and it keeps its own top-up and payment records across restarts so a claim in progress
  carries on. It learns whether sessions are in flight from `funding_in_progress`, which reads storage when funding
  has not resumed yet, and asks again on each `funding_session_changed`.
- **Stored sessions are versioned.** Their blob leads with a format number, and the core keeps reading every earlier
  format, so an upgrade keeps the sessions in flight. A blob it cannot read is kept aside rather than overwritten.

A worker keeps running while the user is elsewhere in the app. Running with the app in the background is out of
scope for now.

## Cancel

`cancel_funding` answers `Ended`, `Requested`, `Pending`, `TooLate` or `Refused` (ended, or its overlay is up).

- **No provider serves the session:** it ends as `Failed { Cancelled }`.
- **Before money moved:** the provider gets `Cancel` and stops with `Failed { Cancelled }`. Asked again, the session
  ends as `Failed { Cancelled }` and the worker is let go, so a provider that went silent cannot hold it.
- **After money moved:** a cancel is a request and never ends the session. The provider answers `CancelRefused
  { reason }`, and the session carries on, or, on an inbound session whose credit has not started, `CancelOffer { fee,
  refund, expires_at }`: amounts in the user's balance units, `fee + refund` at most the session amount. The host shows
  the offer from the session's `cancel` and answers it with `confirm_funding_cancel(intent, terms)`, passing the fee and
  refund it showed to accept or `None` to decline. The provider hears `CancelAccepted` or `CancelDeclined`; while an
  offer is open it is not sent `Cancel` again. The answer is `Changed` when the provider replaced the offer since the
  host showed it, and nothing is accepted; `TooLate` when the credit started meanwhile, and the offer is declined; and
  `Lapsed` past `expires_at`. A user who declined or was refused may ask again. On `CancelAccepted` the provider returns
  the user's payment and ends the session with `Failed`. An outbound session is refunded only once refunds of a released
  withdrawal exist, so until then its provider can only refuse.
- **A payout that lands after an accepted cancel wins.** The withdrawal ends as paid out, and the session keeps
  the terms the user accepted as `cancel: Overtaken { fee, refund }`, so its status reads `PaidOutAfterCancel` and
  the host can say the cancel came too late without remembering it itself.
- **Inbound after its credit started:** too late. `cancel_funding` answers `TooLate`, the host tells the user, and the
  provider is not asked.

## Trade-offs

- **Every provider is asked for every quote.** Manifests lag behind what a provider serves, so routes only narrow the
  list the host draws before quotes arrive, never which provider can be chosen; the cost is one ask per provider per
  quote.
- **The worker holds the provider's flow.** Ephemeral accounts, conversion and payout live in the worker
  (`deriveEntropy` under the provider's product), so the core needs no XCM, swap or provider-specific code, and signs
  no funding transaction.
- **No backend in the manifest.** A provider signs in to its own API and keeps its tokens with the host, so the
  manifest only says what the provider moves.
