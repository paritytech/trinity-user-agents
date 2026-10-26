---
"@parity/truapi": major
---

Funding for the host's screens: a crypto `FundingQuoteAsk` names its `network`, Worker manifest crypto routes declare `networks`, and candidates carry the `limits` quote refusals showed. `funding_progress` reports `retrying` and an outbound session's `payout`, which the provider reports after release with the new `FundingUpdate.Payout`. `FundingFailure.Refunded` ends an inbound payment the provider returned, and `FundingFailure.Declined` one the user's bank or card issuer refused. The provider reports `Converting` on a withdrawal after release. The chosen quote keeps the crypto `network` it was asked for. A `FundingProviderEntry` marked `bundled` is a provider the host ships itself: the core goes by the host's manifest and never reads dotNS for it.

`funding_progress` follows the funding design: three milestones per direction (`Started`, `Received`, `Added` in; `Started`, `Received`, `Sent` out), a `band` for the line under them, and `last_heard_at_ms` for when the provider last reported. A top-up in its crediting step reads `AddingToBalance`. A top-up the user cancelled before paying takes a payment that arrives within 7 days anyway, opens again and is delivered.
