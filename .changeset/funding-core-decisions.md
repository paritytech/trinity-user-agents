---
"@parity/truapi": minor
---

The core holds the funding rules both native apps share, as facts the hosts present their own way:

- `FundingOptions` carries the routed `rails` (crypto, card, bank order), the `default_rail`, the `asset` an ask
  names, the crypto `tokens` and the core's cached `detected_country`. `FundingSelection` takes the picked country's
  `currency`, and each `FundingNetworkLimits` has a `display_name`, the network's canonical English name.
- A quote request asks only providers whose routes carry the rail. `FundingQuoteList` carries the `amount` it prices,
  the shared `refusal` once no provider quoted, and the `issue` every provider's limits showed for that amount;
  `funding_amount_issue` adds the balance and the amount limits.
- `FundingProgress` carries whether the session can be `cancel`led (again, ending it, only before money moved).
  `FundingSessionSummary` carries the `ended` facts: outcome, payout state, amounts and the step a failure stopped
  before.
- `funding_fee_breakdown` (a withdrawal's fee also rounded up to the cent), `funding_units`, `funding_asset_is_fiat`,
  `funding_network_name`, `funding_status_in_flight` and `funding_status_delayed` share the arithmetic and
  classification.
