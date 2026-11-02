---
"@parity/truapi": patch
---

`make e2e-funding-cli` runs the product and the provider's worker as separate processes: the signing host offers the
provider `TRUAPI_FUNDING_PROVIDERS` names, with the worker script after `=`, and starts that worker on demand when the
core asks for it, for a quote or a session, and stops it once the core lets it go. The worker quotes, serves each
session by its direction and amount, answers cancels (an offer on a withdrawal waits for its release, then refunds),
reports a payout through retries and its references, and completes its top-ups and payment requests on the host's
ledger engines; the battery resumes an inbound session after a host restart.
