---
"@parity/truapi": major
---

A funding session keeps the quote, rail and asset it was chosen on, and every `funding_session_changed` hands a native host its progress, the steps for that direction and rail with when each was reached, so the host never reads it back. Providers report their transaction id and reference with the new `FundingUpdate.Details`, at any point while the session is open.
