---
title: "Funding Modality"
type: design
status: draft
---

# Funding Modality

_How value moves into and out of the user's CASH balance. Rationale: [RFC](../rfcs/funding-modality.md) ·
[product manifest](../rfcs/product-manifest.md#funding-configuration) · for provider teams:
[Building a funding provider](../guides/funding-provider.md)._

_Screens are from the Pocket R2 design in Figma; the apps follow it, with small differences._

## Parts

```mermaid
flowchart LR
  product["Product<br/>funding.request · statusSubscribe"]

  subgraph HOST["Host app · iOS / Android / CLI"]
    overlay["Overlay<br/>amount · providers · summary"]
    frame["Provider frame<br/>App route or https page"]
    purse["Purse<br/>session sheet · history"]
    wallet["Coin wallet<br/>top-ups · payments"]
  end

  subgraph CORE["Core · truapi"]
    sessions["Sessions<br/>expiry · cancel · compaction"]
    facts["Facts the hosts share<br/>options · quotes · progress · summary"]
    db[("core.sqlite3")]
    service["FundingProvider service<br/>report checks · settlement"]
  end

  subgraph WORKER["Provider worker"]
    flow["Session flow<br/>rail · conversion · payout"]
  end

  product --> sessions
  sessions <--> db
  sessions --> facts
  facts -- "funding_changed → read back" --> overlay & purse
  overlay -- "selection · pick · cancel" --> sessions
  service <--> flow
  service -- present / close --> frame
  flow -- "payment.topUp / request<br/>(funding: intent)" --> wallet
  wallet -- statuses --> service --> sessions
```

| Who      | Owns                                               | Never                      |
| -------- | -------------------------------------------------- | -------------------------- |
| Host     | screens and their wording, coins, the device       | decides an outcome         |
| Core     | sessions, quotes, outcome, history, the facts      | moves funds, words a label |
| Provider | rail, conversion, payout, its own screens and keys | declares success           |

## Interfaces

```mermaid
flowchart LR
  P[Product] -- "Funding · 22<br/>request · statusSubscribe" --> C[Core]
  C -- "FundingProvider · 24<br/>quote · assign · cancel · settleCancel" --> W[Worker]
  W -- "report · presentFrame · closeFrame" --> C
  W -- "Payment · 9<br/>topUp · request (funding: intent)" --> C
  C -- "callbacks" --> H[Host]
  H -- "actions · reads" --> C
```

| Host ↔ core (native names) | Methods                                                                                                                                                                                                                                                                                 |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Callbacks (core → host)    | `present_funding` · `provider_frame_requested` · `present_provider_frame` · `close_provider_frame` · `detected_country` · `funding_changed(intent, kind)`                                                                                                                                                            |
| Actions (host → core)      | `set_funding_provider_config` · `open_funding` · `open_provider_frame` · `select_funding_options` · `get_funding_quote` · `start_funding_when_quoted` · `cancel_funding_auto_start` · `select_funding_provider` · `cancel_funding` · `confirm_funding_cancel` · `acknowledge_funding_session` · `worker_exited` |
| Reads (host → core)         | `funding_session` · `funding_progress` · `funding_options` · `funding_quotes` · `funding_amount_issue` · `funding_entries` · `funding_summary` · `funding_history` · `funding_available` · `funding_resume_pending`                                                                     |

Session id: `fs_` + nanoid(10). Another product's session reads as `NotFound`. Amounts cross the FFI as decimal strings.

### The host loop

The core never pushes state. It says what changed; the host reads it back and draws it.

```mermaid
sequenceDiagram
  participant C as Core
  participant H as Host
  C->>C: commit (stored before anyone hears)
  C->>H: funding_changed(intent, kind)
  alt Session
    H->>C: funding_session · funding_progress · funding_entries
  else Options
    H->>C: funding_options · funding_amount_issue
  else Quotes
    H->>C: funding_quotes
  else Providers (intent = None)
    H->>C: funding_available
  end
  H->>H: word and draw it
```

## Top-up

<p>
  <img src="funding-modality/amount-in.png" width="180" alt="Top up: amount and rail">
  <img src="funding-modality/providers.png" width="180" alt="Choose a provider">
  <img src="funding-modality/summary.png" width="180" alt="Summary before paying">
  <img src="funding-modality/crypto-deposit.png" width="180" alt="Crypto deposit">
</p>

```mermaid
sequenceDiagram
  participant P as Product
  participant H as Host
  participant C as Core
  participant W as Provider
  P->>C: request(In, amount?)
  C-->>P: intent
  Note over C,W: every candidate's worker held while the overlay is up
  C->>H: present_funding
  H->>C: select_funding_options(rail, asset, network, country)
  H->>C: get_funding_quote(intent, amount)
  C->>W: quote (each provider routing the rail, 10 s)
  W-->>C: Quoted | Refused
  C->>H: funding_changed(Quotes) · re-asked every 60 s
  H->>C: select_funding_provider (or start_funding_when_quoted)
  C->>C: store the session · other quotes end
  C->>W: assign
  opt provider screen
    W->>C: presentFrame
    C->>H: provider_frame_requested · held
    alt user on the session
      H->>C: open_provider_frame
    else user elsewhere
      H->>H: notice "<provider> wants to show a screen"
      H->>C: open_provider_frame on tap
    end
    C->>H: present_provider_frame
    W->>C: closeFrame → host shows the session sheet
  end
  W->>C: AwaitingPayment · PaymentReceived · Converting
  W->>C: payment.topUp(id, amount, funding: intent)
  C->>H: claim into CASH
  W->>C: Crediting(id, amount) · Delivered
  C->>C: settle from the final claim
  C->>H: funding_changed(Session)
  H->>C: acknowledge_funding_session (once the user saw it)
  C->>P: Delivered(credited)
```

## Withdrawal

<p>
  <img src="funding-modality/amount-out.png" width="180" alt="Send money out: amount and rail">
</p>

```mermaid
sequenceDiagram
  participant H as Host
  participant C as Core
  participant W as Provider
  C->>W: assign (Out)
  W->>C: payment.request(id, amount, funding: intent)
  C->>H: user approves the debit
  W->>C: Collecting(id, amount)
  Note over C: Released (debited)
  W->>C: Converting · PayoutRetrying* · Payout
  alt payout failed
    W->>C: payment.topUp refund (funding: intent)
    W->>C: Refunding(top_up_id, amount)
    Note over C: PayoutFailed { refunded }
  end
```

## Session lifecycle

<p>
  <img src="funding-modality/in-progress.png" width="180" alt="Sessions in progress on the Purse">
</p>

```mermaid
stateDiagram-v2
  [*] --> Presenting: request
  Presenting --> [*]: dismissed → Failed{Cancelled}
  Presenting --> Open: user starts (stored)
  Open --> Assigned: provider picked
  Open --> Expired: 24 h
  Assigned --> Expired: 7 d silent, no money moved
  Assigned --> Failed: provider Failed
  Assigned --> MoneyMoved: named funds held
  MoneyMoved --> Delivered: In
  MoneyMoved --> Released: Out
  Released --> Sent: Payout PaidOut
  Released --> PayoutFailed: Payout Failed + refund
  Delivered --> [*]
  Sent --> [*]
  PayoutFailed --> [*]
  Failed --> [*]
  Expired --> [*]
```

Once money moved, only the provider ends a session. A bank top-up cancelled before paying stays open 7 days for a late
transfer, which is still added.

## Cancel

<p>
  <img src="funding-modality/cancel-top-up.png" width="180" alt="Cancel a pending top-up">
</p>

```mermaid
flowchart TD
  c[user cancels] --> assigned{provider assigned?}
  assigned -- no --> none[Cancelled]
  assigned -- yes --> mm{money moved?}
  mm -- no --> stop[cancel → Stopped] --> none
  mm -- yes --> late{credit started?}
  late -- yes --> toolate[TooLate]
  late -- no --> req[cancel call]
  req --> refused[Refused · carries on]
  req --> offer[Offer fee / refund]
  offer --> confirm{confirm_funding_cancel}
  confirm -- declined --> on[settleCancel Declined · carries on]
  confirm -- accepted --> refund[settleCancel Accepted → refund]
  refund -. payout landed first .-> paid[PaidOutAfterCancel]
```

A cancel that moved no money is left out of history.

## Funds a worker moves

```mermaid
flowchart LR
  call["payment.topUp / request<br/>funding: intent"] --> check{serves the session?<br/>direction · ≤3 per session}
  check -- no --> nf[FundingNotServed]
  check -- yes --> pending[Pending, stored before the call]
  pending --> accepted[Accepted once the host holds it]
  pending -. "1 h, engine doesn't hold it" .-> gone[dropped]
  accepted --> named["named once in a report<br/>Crediting · Collecting · Refunding"]
  named --> settle[counts toward the outcome when final]
```

A repeat under the same id must ask the same thing.

## Storage

<p>
  <img src="funding-modality/history.png" width="180" alt="History on the Purse">
</p>

```mermaid
flowchart LR
  subgraph db["core.sqlite3 · core-owned"]
    s[(funding_session · format 5<br/>in flight · payout owed · late arrival open)]
    k[(funding_summary · format 3<br/>every other ended session · acknowledged flag)]
  end
  subgraph kv["Core storage · host-backed"]
    l[(FundingSupport<br/>learned limits · 12 h)]
    m[(WorkerManifest cache · 24 h)]
  end
  s -- "ends · same transaction" --> k
  s & k -- funding_history --> hv[Host history]
  hv -. acknowledge_funding_session .-> k
```

| Row read            | Result                                           |
| ------------------- | ------------------------------------------------ |
| current format      | read                                             |
| older / undecodable | skipped, kept, never written over                |
| newer (downgrade)   | load fails, nothing touched, funding unavailable |

A wasm core has no database and keeps no session.

## Restart and worker crash

```mermaid
sequenceDiagram
  participant H as Host
  participant C as Core
  participant W as Provider
  H->>C: funding_resume_pending? → start funding at launch
  H->>C: set_funding_callbacks → open core.sqlite3
  C->>C: load sessions + summaries, expire overdue
  C->>H: funding_changed(Session) per session in flight · 200 newest unacknowledged
  C->>H: request_worker_transition(Start) for each held provider
  W->>C: handshake
  C->>W: assign(last update) per session, then unanswered cancels
  W->>W: resume from its saved state
  Note over H,W: a crash later: worker_exited → Start again while a session or the overlay holds it (no backoff)
```

## Numbers

| What                              | Value       |
| --------------------------------- | ----------- |
| quote deadline · refresh          | 10 s · 60 s |
| learned limits, refused country   | 12 h        |
| detected country reuse · retry    | 1 h · 1 min |
| open session expires              | 24 h        |
| assigned, silent, no money moved  | 7 d         |
| late bank transfer after a cancel | 7 d         |
| provider call timeout             | 60 s        |
| unaccepted funds kept             | 1 h         |
| top-ups or payments per session   | 3           |
| `Details` · `Deposit` per session | 8 · 8       |
| unacknowledged re-announced       | 200 newest  |

## Test

`make e2e-funding-cli`: the product and provider battery against the CLI host.
