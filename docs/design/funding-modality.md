---
title: "Funding Modality"
type: design
status: draft
---

# Funding Modality

_How value moves into and out of the user's CASH balance, across product, core, host and provider. Rationale:
[RFC — Funding modality](../rfcs/funding-modality.md), [product manifest](../rfcs/product-manifest.md#funding-configuration)._

## Parts

```mermaid
flowchart LR
  product["Product<br/>funding.request · statusSubscribe"]

  subgraph HOST["Host app · iOS / Android"]
    overlay["Overlay<br/>amount · providers · summary · deposit · cancel"]
    frame["Provider frame<br/>relative App route or https page"]
    purse["Purse<br/>dock · Total panel · history"]
    wallet["Coin wallet<br/>top-ups · payments · balance"]
    secure["Secure storage<br/>auth tokens"]
  end

  subgraph CORE["Core · truapi"]
    sessions["Sessions<br/>store · expiry · cancel · pack"]
    quotes["Discovery + quotes"]
    provider["FundingProvider service<br/>report checks"]
    settle["Settlement"]
    payments["Payments by id"]
  end

  subgraph WORKER["Provider worker"]
    flow["Session flow<br/>rail · conversion · payout"]
  end

  product --> sessions
  sessions <--> overlay
  sessions --> purse
  quotes <--> flow
  provider <--> flow
  provider --> frame
  flow --> payments
  payments <--> wallet
  provider --> settle --> sessions
  flow <--> secure
```

| Who | Owns | Never |
|---|---|---|
| Host | screens, coins, secrets, history | decides an outcome |
| Core | sessions, quotes, outcome | moves funds |
| Provider | rail, conversion, payout | declares success |

## Interfaces

| Trait | Id | Methods | Caller |
|---|---|---|---|
| `Funding` | 22 | `request({ id, direction, amount? })` · `statusSubscribe({ id })` | product |
| `FundingProvider` | 24 | core → worker: `quote` · `assign` · `cancel` · `settleCancel`; worker → core: `report` · `presentFrame` | core and provider worker |
| `Payment` | 9 | `topUp` · `topUpStatusSubscribe` · `request` · `statusSubscribe` · `balanceSubscribe` | provider worker |
| `AuthTokens` | 27 (#1553) | `store` · `get` · `remove` | provider worker |
| `FundingPlatform` | host | `present_funding` · `present_provider_frame` · `detected_country` · `funding_session_changed` · `funding_quotes_changed` | core → host |

Session id: `fs_` + hex(blake2("truapi/funding-id", product, id)). Reused id → `AlreadyExists`. Host-opened: `fs_` + nanoid(10).

Worker payments name their session (`funding`). A session the worker doesn't serve, the wrong direction, or a full session (≤3 top-ups/payments) → `FundingNotServed`. A repeat under the same id must ask the same.

Started funds are recorded Pending before the call and Accepted once the host holds them. Only Accepted funds can be named in a report, and only once. Unaccepted ones go after 1 h if the engine doesn't hold them.

```mermaid
flowchart LR
  core["core asks · one answer item each"] --> Q["quote → Quoted | Refused"]
  core --> A["assign → ack"]
  core --> C["cancel → Stopped | Offer | Refused"]
  core --> S["settleCancel(Accepted | Declined) → ack"]
  report["worker reports"] --> u1[AwaitingPayment · PaymentReceived · Converting]
  report --> u2[Crediting · Delivered · Collecting]
  report --> u3[Payout · PayoutRetrying · Refunding]
  report --> u4[Deposit · Details · Failed]
```

## Top-up

```mermaid
sequenceDiagram
  participant P as Product
  participant H as Host
  participant C as Core
  participant W as Provider
  P->>C: request(id, In, amount?)
  C->>H: present_funding
  H->>C: rail, asset, country
  C->>W: quote (every provider, 10 s deadline)
  W-->>C: Quoted
  C->>H: funding_quotes_changed: rows Pending → Quoted | Unavailable(Refused or Timeout), best · re-asked every 60 s
  H->>C: select_funding_provider
  C->>W: assign
  W->>C: presentFrame
  C->>H: provider frame
  W->>C: AwaitingPayment, PaymentReceived, Converting
  W->>W: keep state, then payment.topUp(id, amount, funding: intent)
  C->>H: claim into CASH
  W->>C: Crediting(id, amount), Delivered
  Note over C: settles from final claims
  C->>H: ended → history, acknowledge
  C->>P: Delivered(credited)
```

## Withdrawal

```mermaid
sequenceDiagram
  participant H as Host
  participant C as Core
  participant W as Provider
  C->>W: assign (Out)
  W->>W: keep state, then payment.request(id, amount, funding: intent)
  C->>H: user approves debit
  W->>C: Collecting(id, amount)
  Note over C: Released (debited)
  W->>C: PayoutRetrying* → Payout
  alt payout failed
    W->>C: payment.topUp refund (funding: intent)
    W->>C: Refunding(top_up_id, amount)
    Note over C: PayoutFailed { refunded }
  end
```

## Session lifecycle

```mermaid
stateDiagram-v2
  [*] --> Presenting: request
  Presenting --> [*]: dismissed → Failed{Cancelled} to watchers
  Presenting --> Open: user starts (stored)
  Open --> Assigned: select provider
  Open --> Expired: 24 h
  Assigned --> Expired: 7 d silent, no money moved
  Assigned --> MoneyMoved: started · named · payment held
  MoneyMoved --> Delivered: in
  MoneyMoved --> Released: out
  Released --> Sent: Payout PaidOut
  Released --> PayoutFailed: Payout Failed + refund
  Assigned --> Failed: provider Failed
  Delivered --> [*]
  Sent --> [*]
  PayoutFailed --> [*]
  Failed --> [*]
  Expired --> [*]
```

The overlay runs on its own task: a request that goes away still starts or discards its session.

One serialized reconciler, woken on every change, runs the background work: load or sweep, then retain quotes · hold workers · follow settlements · resolve starts · catch up links. Sweep retry 30 s, catch-up retry 5 s, provider call timeout 60 s.

## Cancel

```mermaid
flowchart TD
  c[user cancels] --> assigned{provider assigned?}
  assigned -- no --> none[ended: Cancelled]
  assigned -- yes --> mm{money moved?}
  mm -- no --> first{first ask?}
  first -- yes --> ask[cancel call] --> stopped[Stopped → ended: Cancelled]
  first -- no --> ended[ended: Cancelled]
  mm -- yes --> late{credit started?}
  late -- yes --> toolate[TooLate]
  late -- no --> req[cancel call]
  req --> refused[Refused · carries on]
  req --> offer[Offer fee/refund · one per cancel]
  offer --> confirm{confirm_funding_cancel}
  confirm -- declined --> on[settleCancel Declined · carries on]
  confirm -- accepted --> refund[settleCancel Accepted → Refunding · Failed Refunded]
  refund -. payout landed first .-> paid[PaidOutAfterCancel]
```

## Storage

```mermaid
flowchart LR
  subgraph core["Core storage"]
    s[(FundingSessions<br/>in flight · ≤200 unrecorded · ≤64 recorded/product)]
    k[(FundingSessionPack<br/>compacted, never trimmed)]
    l[(FundingSupport<br/>learned limits · 12 h)]
    m[(WorkerManifest cache)]
    u[(FundingSessionsUnreadable)]
  end
  subgraph host["Host storage"]
    h[(History store)]
    t[(Secure tokens)]
  end
  s -- overflow --> k
  s -- ended --> h
  h -- acknowledge --> s
  k -- funding_history --> h
  s -- corrupt blob --> u
```

Sessions blob format 2, pack format 1. Each blob leads with a u16 format.

| Blob | Format rule |
|---|---|
| current format | read |
| older | cleared, start empty |
| corrupt | set aside (≤8), start empty |
| newer (downgrade) | left untouched, funding unavailable |

## Rows

Title is the amount (`$50 CASH` in flight, `+$50 CASH` added, `-$50 CASH` out), rail pill on the right, subtitle below.
History: In progress · Completed.

| Status | Subtitle |
|---|---|
| WaitingForPayment / WaitingForDeposit / WaitingForTransfer | Waiting for your payment / deposit / transfer |
| ConvertingToCash · AddingToBalance | On its way · Adding to your balance |
| Retrying | The provider is retrying. Don't pay again (amber) |
| TransactionInitiated | Withdrawal initiated |
| ConvertingTo / PreparingPayout | Converting to {asset}… / Preparing your payout |
| RetryingPayout | The provider is retrying the payout (amber) |
| TakingLonger | Delayed, don't pay again · out: Taking longer than usual (amber) |
| ToppedUp / Sent / PaidOutAfterCancel | {moment} |
| Refunded / PayoutFailed / Cancelled | Refunded / Payout failed / Cancelled · {moment} |
| CancelRequested · CancelOffered · CancelRefused · Cancelling | Cancel requested, waiting for the provider · Cancel offer waiting for you (amber) · The provider can't cancel (amber) · Cancelling… |
| Failed | Expired · {moment} / Payment failed · {moment} |

## Session sheet

```
        ⟳
     $50 CASH
 ●─•─•─●─•─•─○        Started · Received · Added      (out: Received by provider · Sent)
 Checking your payment            band
 Checked with the provider 3s ago  last_heard_at_ms · just now / s / min / h
 Send this exact amount inc. fees   €50.55 ›
 Reference                    5729 4183 6602 ⧉
 Arrives  1–2 business days · Final Cash depends on the rate on arrival
 [ Cancel ]   pending bank top-up → Cancel top-up? · Cancel / Keep it
```

| Band | Line |
|---|---|
| AwaitingPayment | Waiting for your payment · bank: "If you've sent the money from your bank, it's on its way to us…" |
| CheckingPayment · OnItsWay | Checking your payment · Your money is on its way |
| Adding · Added | Adding $50 CASH to your balance · $50 CASH added to your balance |
| Converting · PreparingPayout · Sending · Sent | Converting your Cash · The provider is preparing your payout · Sending the money to your {rail} · Sent to your {rail} |
| crypto: CheckingPayment · PreparingPayout · Sending · Sent | Confirming your {asset} deposit · Exchanging your money for {asset} · Sending your {asset} · Your {asset} is on its way to your wallet |
| Retrying · Delayed | amber, "don't pay again" on top-ups |
| Cancel · Ended | no line |

A bank top-up cancelled before paying takes a transfer that arrives within 7 days: it opens again and is added.

## Purse

```
┌ Purse ─────────────────────┐
│ [ leather card  $X CASH ]  │
│ [ ↑ Get ]      [ ↓ Send ]  │   Get: Topping up sheet if any in progress, else amount
│ Total $X CASH  ●●●●●●      │   Send: contacts · Send money out → Sending sheet / amount
│ ▒▒▒░░░░░░   Ready $X       │   ⟲ clock → History
└────────────────────────────┘
```

## Restart

```mermaid
sequenceDiagram
  participant H as Host
  participant C as Core
  participant W as Provider
  H->>C: launch (resume marker set) → start core
  C->>C: load sessions + pack
  C->>H: re-announce unacknowledged
  W->>C: handshake (worker connects)
  C->>W: assign(last update) per session it serves, then unanswered cancel calls
  W->>W: continue from the state it kept
```

Catch-up is one call at a time per link, retried until every call is answered.

## Test

`make e2e-funding-cli`: the product and provider battery against a scripted CLI host.
