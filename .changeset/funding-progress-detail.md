---
"@parity/truapi": minor
---

A funding provider reports what it is doing within a step with `FundingUpdate::Progress { detail }`: a typed
`FundingFact` (`Checked`, `Confirmations`, `PriceChecked`, `Parts`, `Scheduled`, `SentByBank`, `ExpectedBy`,
`InReview`, `Attempt`) with an optional time, which the host words in the user's language, or as a last resort an
`Other` text of at most 80 characters shown as given. The session's progress carries the latest as `detail` until the
step moves on; each one counts as hearing from the provider and none is kept in the history. A fact that does not fit
the session fails with `InvalidDetail`.
