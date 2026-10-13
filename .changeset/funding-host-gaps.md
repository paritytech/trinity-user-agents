---
"@parity/truapi": major
---

Funding for the host's screens: a crypto `FundingQuoteAsk` names its `network`, Worker manifest crypto routes declare `networks`, and candidates carry the `limits` quote refusals showed. `funding_progress` reports `retrying` and an outbound session's `payout`, which the provider reports after release with the new `FundingUpdate.Payout`. `FundingFailure.Refunded` ends an inbound payment the provider returned, and `FundingFailure.Declined` one the user's bank or card issuer refused. The progress steps follow the funding screens: card and crypto top-ups pass `Approved`, a bank top-up goes from `Payment` to `Added`, and a withdrawal runs on through `Conversion` (the provider reports `Converting` after release) to the new `Payout` step. The chosen quote keeps the crypto `network` it was asked for. A `FundingProviderEntry` marked `bundled` is a provider the host ships itself: the core goes by the host's manifest and never reads dotNS for it.
