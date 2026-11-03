# Building a funding provider

A funding provider moves money into or out of a user's CASH balance: a card or bank payment, or a crypto deposit,
becomes CASH (inbound), or CASH becomes a payout to the user's bank, card or wallet (outbound). This guide is for teams
building one. The [RFC](../rfcs/funding-modality.md) is the full contract, and the
[design doc](../design/funding-modality.md) shows what the user sees.

## What you build

A provider is a TrUAPI product with a Worker, which answers quotes and runs each session: it talks to your API, moves
funds and reports progress. Pick one of three shapes for the screens the user sees:

| Shape               | What you publish                                            | How the Worker shows a screen                                                            |
| ------------------- | ----------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| **Worker only**     | the Worker                                                  | it shows none; the host's session sheet is enough, as for a crypto deposit to an address |
| **App on dotNS**    | the Worker and an App, both under your product's dotNS name | `App { route }`: the host opens your App at that route, as a product                     |
| **Self-hosted app** | the Worker; your screens stay on your own https site        | `Page { url, returnUrl }`: the host shows the site with its address visible              |

An `App` route always means your own product's App on dotNS, never another product's or an outside site; a Worker-only
or self-hosted provider that sends one gets `NoApp`.

You do not build the overlay, the provider list, the session sheet or the history: the host draws them from what your
Worker reports, in the user's language. Your Worker never decides whether a session succeeded either: the core does,
from the top-ups and payments it names.

```
user picks amount ──▶ core asks every provider ──▶ your Worker: quote
user picks you    ──▶ core assigns the session ──▶ your Worker: assign
                                                   your Worker: present screens, take payment,
                                                                report steps, move funds
core follows the funds it named ──▶ session Delivered / Released / Failed
```

## 1. Declare that your Worker serves funding

In the Worker manifest, `includes.funding` says you are a provider. `routes` are optional and only let the host show
what you serve before quotes arrive; your quotes are what count.

```json
{
  "$v": 1,
  "kind": "worker",
  "entrypoint": "index.js",
  "includes": {
    "funding": {
      "routes": [
        { "mode": "CARD", "directions": ["In"], "assets": ["EUR", "USD"], "countries": ["DE", "FR", "US"] },
        { "mode": "CRYPTO", "directions": ["In", "Out"], "assets": ["USDT"], "networks": ["polkadot", "ethereum"] }
      ]
    }
  }
}
```

One mode per route; assets are uppercase ISO 4217 codes for card and bank and token symbols for crypto; countries are
ISO 3166-1 alpha-2. A host offers the providers it bundles or resolves by their published names, so getting listed is
agreed with each host.

## 2. Register the four handlers

The core calls your Worker; each handler returns one answer. Register them as soon as the Worker starts:

```ts
const client = await truapi();
client.fundingProvider.onQuote(({ ask }) => ok(price(ask)));
client.fundingProvider.onAssign(({ session }) => {
  run(session);
  return ok(undefined);
});
client.fundingProvider.onCancel(({ intent }) => answerCancel(intent).then(ok));
client.fundingProvider.onSettleCancel(({ intent, decision }) => settle(intent, decision).then(() => ok(undefined)));
```

Calls arrive in no order, and the same one can arrive twice, so look sessions up by `intent` and make every handler safe
to repeat.

## 3. Quote

`ask` names the direction, rail, asset, network (crypto), amount in balance units and the user's country. Answer within
10 seconds:

```ts
{ tag: "Quoted", value: { quote: { quoteId, sendAmount, receiveAmount, providerFee, networkFee, etaSecs, expiresAt, assetDecimals } } }
{ tag: "Refused", value: { reason: { tag: "BelowMinimum", value: { min } } } }   // or CountryUnsupported, AboveMaximum, Unavailable, Other
```

- Inbound, `receiveAmount` is the CASH credited and must equal the ask's amount; outbound, `sendAmount` is the CASH
  debited and must equal it.
- Both amounts are above zero, `providerFee + networkFee` is at most `sendAmount`, `quoteId` is at most 128 characters
  with no control characters, and `expiresAt`, when set, is still ahead. A quote that breaks any of these shows as you
  being unavailable.
- `etaSecs` matters: once the provider holds the payment, a session silent past it shows as delayed.
- Refusing with `BelowMinimum` or `AboveMaximum` teaches the host your limits for 12 hours.

## 4. Run a session

`assign` hands you a `FundingAssignment`: `intent`, `direction`, `amount`, the chosen `quote`, and `lastUpdate`, the
last report the core stored, which is where you resume after a restart.

Each step is sent in its session's direction: an inbound step as `{ tag: "In", value: step }`, an outbound one as
`{ tag: "Out", value: step }`. `Failed`, `Details` and `Progress` belong to every session and are sent as they are.

```ts
report({ tag: "In", value: { tag: "PaymentReceived", value: { finalized: true } } });
report({ tag: "Out", value: { tag: "Collecting", value: { paymentId, amount } } });
```

**Inbound**

1. `report AwaitingPayment`, and a `Deposit` with the address or bank details the host should draw: an amount above
   zero, by the rail and asset of the quote the user chose, not yet expired, and a payment URI (BIP21, EIP-681) rather
   than a web, script or file one. Send a new `Deposit` until funds move, up to 8 per session.
2. Show your screen if you need one (below), take the payment.
3. `report PaymentReceived { finalized: false }`, then `{ finalized: true }` once it cannot be reversed.
4. `report Converting`.
5. Credit the user: `payment.topUp({ amount, source, id, funding: intent })`, then
   `report Crediting { topUpId, amount }`. A partial claim may be followed by another top-up, up to 3.
6. `report Delivered`.

**Outbound**

1. Ask for the user's CASH: `payment.request({ amount, destination, id, funding: intent })`, then
   `report Collecting { paymentId, amount }`. The core marks the session `Released` once the payment completes.
2. `report Converting`, `PayoutRetrying` as often as an attempt fails, and finally `Payout { PaidOut }` or
   `Payout { Failed }`.
3. After a failed payout, give the CASH back with a `payment.topUp` and `report Refunding { topUpId, amount }`.

**Any time:** `Details { transactionId, reference }` for the references the user can quote to you, up to 8 per session,
and `Failed { reason }` to end a session before any funds move.

Every text you report (a reference, an address, a `Progress` `Other` text) is one plain line of at most 80 characters,
with no control or invisible characters; a report that breaks a limit fails with `InvalidDetail`.

Reports only move forward; a report out of order fails with `OutOfOrder`, and one on a session you no longer serve with
`NotFound`, which means the session is over and you can drop its state.

## 5. Show your screens

```ts
await client.fundingProvider.presentFrame({ intent, target: { tag: "App", value: { route: "/card" } } });
await client.fundingProvider.presentFrame({ intent, target: { tag: "Page", value: { url, returnUrl } } });
```

`App` opens your dotNS App at `route`: a relative path with an optional query and fragment, such as
`/card?intent=fs_abc123`, which your App's router reads. Anything that would leave your App, such as `https://…` or
`//other.dot`, fails with `InvalidUrl`, and an App route from a provider that publishes no App fails with `NoApp`.

`Page` opens your self-hosted https site, such as an onramp widget, with its address shown, and hands back the
`returnUrl` it reached, query included, so the Worker can read the result from it. A page cannot call TrUAPI: the Worker
still takes the result and moves the funds. `presentFrame` answers once the frame closes: `Closed`, `Dismissed` by the
user, or `Returned { url }`. When the user has done what the screen was for, call
`fundingProvider.closeFrame({ intent })`: the host closes it and shows the session sheet instead.

## 6. Say what is happening within a step

While a step takes time, report a `Progress` fact for the line under the session's status. The host words it, so you
never need the user's language. Send one whenever you check; each counts as hearing from you, and none is kept in the
history.

```ts
report({
  tag: "Progress",
  value: { detail: { fact: { tag: "Confirmations", value: { done: 2, required: 3 } }, atMs: lastBlockMs } },
});
```

| Fact                               | Shown as                                              | `atMs`              |
| ---------------------------------- | ----------------------------------------------------- | ------------------- |
| `Checked`                          | Checked with the provider 12s ago                     | when you checked    |
| `Confirmations { done, required }` | Confirmation 2 of 3 · last block 4s ago (crypto only) | the last block      |
| `PriceChecked`                     | Price checked · 3s ago                                | when you checked    |
| `Parts { done, total }`            | Part 2 of 4 · last part 5s ago                        | the last part       |
| `Scheduled`                        | About 12s left                                        | when it is due      |
| `SentByBank`                       | Sent by your bank · Tue 14:02 (inbound bank)          | when it was sent    |
| `ExpectedBy`                       | Expected by Thu 3 Nov                                 | the expected day    |
| `InReview`                         | The provider is reviewing the payment                 | when review started |
| `Attempt { n, of }`                | Attempt 2 of 3 · next try in 20s                      | the next try        |
| `Other { text }`                   | your text, untranslated                               | when it happened    |

Use `Other` only when no fact fits: at most 80 characters of plain text on one line. A fact that does not fit the
session fails with `InvalidDetail`.

## 7. Cancels

`onCancel` answers `Stopped` before money moved, and the session ends. After money moved a cancel is a request: answer
`Refused { reason }`, or offer `Offer { fee, refund, expiresAt }` (inbound before the credit, outbound after release).
`onSettleCancel` then tells you `Accepted { fee, refund }` or `Declined`. On acceptance, return exactly `refund`: end an
inbound session with `Failed { Refunded }`, or refund a withdrawal with `Refunding`.

## 8. Survive restarts

The app can be closed and your Worker can crash at any point. The core keeps every session and report, starts your
Worker again and assigns it every session it still serves. Your part:

- **Save before every external action** (a call to your API, `payment.topUp`, `payment.request`) in your product's
  `localStorage`, under the session's `intent`. Delete it after your last report.
- **Derive ids from the session and the step**, so a repeated top-up, payment or order after a crash is answered
  `AlreadyExists` instead of moving funds twice.
- **Resume from `lastUpdate` and your saved state**, never from the start.
- **Retry transient failures** of reports and calls, with a growing wait.

Once money moved, only you can end the session: nothing expires it and no cancel ends it, so a provider that stops
reporting holds the session for good.

## 9. Test it

- `make e2e-funding-cli` runs the funding battery against a scripted host.
- Run your sessions against a fake host that takes reports in the core's order and refuses the rest, so an out-of-order
  report fails your tests rather than a user's session.
- Stop the Worker at each step, after the external action and before the next save, and check that the resumed session
  finishes and moves no funds twice.
- Give yourself a way to choose each failure on testnet, such as a declined payment, a refund, a retried payout or a
  late arrival, so every ending can be run on a device.

## Checklist

- [ ] `includes.funding` in the Worker manifest, routes matching what you quote
- [ ] One screen shape: Worker only, an App on dotNS, or a self-hosted site
- [ ] Four handlers registered at start, each safe to repeat
- [ ] Quotes within 10 s, with `etaSecs`, refusals with limits
- [ ] Every top-up and payment passes `funding: intent` and is named in a report
- [ ] State saved before every external action; ids derived, so repeats are safe
- [ ] `closeFrame` once the user acts on your screen
- [ ] `Progress` facts during long steps
- [ ] Cancel answers, and offers refunded exactly
- [ ] Every session ends with your last report: `Delivered`, `Failed`, `Payout` and any `Refunding`
