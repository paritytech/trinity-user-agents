---
"@parity/truapi": major
---

A funding session keeps the quote, rail and asset it was chosen on, and `funding_progress(intent)` gives hosts its steps for that direction and rail with when each was reached. Providers report their transaction id and reference with the new `FundingUpdate.Details`, at any point while the session is open, and keep their own state for a session with the new `FundingProvider.save`, handed back in `Assigned.saved` after a restart.
