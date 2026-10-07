---
"@parity/truapi": major
---

`get_funding_quote(intent, ask)` prices a funding ask with every candidate's worker: core sends a `Quote` item on `serveSubscribe` and the worker answers with the new `FundingProvider.answerQuote`. Rows resolve one by one, a provider that does not answer in 10 seconds is unavailable, and recent answers are reused. `select_funding_provider` takes the chosen quote id and the provider receives the quote in `Assigned`. Hosts implement `funding_quote_changed` on `NativeFundingCallbacks`.
