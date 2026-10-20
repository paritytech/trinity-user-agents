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
    frame["Provider frame<br/>App route or https page"]
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
| `FundingProvider` | 24 | `serveSubscribe` · `answerQuote` · `report` · `presentFrame` · `save` | provider worker |
| `Payment` | 9 | `topUp` · `topUpStatusSubscribe` · `request` · `statusSubscribe` · `balanceSubscribe` | provider worker |
| `AuthTokens` | 26 | store · read | provider worker |
| `FundingPlatform` | host | `present_funding` · `present_provider_frame` · `detected_country` · `funding_session_changed` | core → host |

Session id: `fs_` + hex(blake2("truapi/funding-id", product, id)). Reused id → `AlreadyExists`.

```mermaid
flowchart LR
  serve["serveSubscribe items"] --> Q[Quote] & A[Assigned] & C[Cancel] & CA[CancelAccepted] & CD[CancelDeclined]
  report["report updates"] --> u1[AwaitingPayment · PaymentReceived · Converting]
  report --> u2[Crediting · Delivered · Collecting]
  report --> u3[Payout · PayoutRetrying · Refunding]
  report --> u4[Deposit · Details · Failed]
  report --> u5[CancelOffer · CancelRefused]
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
  C->>W: Quote (every provider)
  W-->>C: answerQuote
  C->>H: quote list (request id, best)
  H->>C: select_funding_provider
  C->>W: Assigned
  W->>C: presentFrame
  C->>H: provider frame
  W->>C: AwaitingPayment, PaymentReceived, Converting
  W->>C: save, then payment.topUp(id, amount)
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
  C->>W: Assigned (Out)
  W->>C: save, then payment.request(id, amount)
  C->>H: user approves debit
  W->>C: Collecting(id, amount)
  Note over C: Released (debited)
  W->>C: PayoutRetrying* → Payout
  alt payout failed
    W->>C: payment.topUp refund
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

## Cancel

```mermaid
flowchart TD
  c[user cancels] --> mm{money moved?}
  mm -- no --> first{first ask?}
  first -- yes --> ask[ask provider] --> wait[CancelRequested]
  first -- no --> ended[ended: Cancelled]
  mm -- yes --> late{top-up credited?}
  late -- yes --> toolate[TooLate]
  late -- no --> req[ask provider]
  req --> refused[CancelRefused · carries on]
  req --> offer[CancelOffer fee/refund]
  offer --> confirm{confirm_funding_cancel}
  confirm -- terms changed --> changed[Changed]
  confirm -- declined --> on[carries on]
  confirm -- accepted --> refund[Refunding · Failed Refunded]
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
  s -- corrupt/older blob --> u
```

| Blob | Format rule |
|---|---|
| current format | read |
| older or corrupt | set aside, start empty |
| newer (downgrade) | left untouched, funding unavailable |

## Rows

Title · subtitle. Ended subtitle: `{Rail} · {moment}`.

| Status | Row |
|---|---|
| WaitingForPayment / WaitingForDeposit | Topping up · Upcoming receiving |
| WaitingForTransfer | Topping up · Waiting for transfer |
| ConvertingToCash | Topping up · Converting to Cash |
| Retrying | Topping up · Provider is retrying your payment (amber) |
| TransactionInitiated | Sending · Transaction initiated |
| PreparingPayout / ConvertingTo | Sending · Preparing payout / Converting to {asset} |
| RetryingPayout | Sending · Provider is retrying the payout (amber) |
| TakingLonger | · Taking a little longer (amber) |
| ToppedUp / Sent / PaidOutAfterCancel | Topped up / Sent · {Rail} · {moment} |
| PayoutFailed / Cancelled / Refunded | Payout failed / Cancelled / Refunded · {Rail} · {moment} |
| Failed | Top-up failed / Withdrawal failed · reason |

Source of truth: `FundingStatus` docs in `host_logic/funding.rs`.

## Purse

```
┌ Purse ─────────────────────┐   scroll: card → 60pt thumbnail in header
│ [ leather card  $X CASH ]  │            + sticky "Current balance" bar
│ [ Get ]        [ Send ]    │
│ Total $X CASH  ●●●●●●      │   Clearing / Ready from coinage
│ ▒▒▒░░░░░░  Clearing · Ready│
│ Today                      │
│  ○ Topped up       +$25    │   history by day
│  ○ Sent            -$50    │
└────────────────────────────┘
  ┌ in progress dock ──────┐     front: delayed, else newest
  │ (2) Topping up   +$50  │     tap → panel
  └────────────────────────┘
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
  C->>W: serveSubscribe replays Assigned(last update, saved)
  W->>W: continue from saved checkpoint
```

## Test

`make e2e-funding-cli`: the product and provider battery against a scripted CLI host.
