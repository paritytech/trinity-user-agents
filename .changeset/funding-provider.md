---
"@parity/truapi": minor
---

New `FundingProvider` trait for a provider's worker: `serveSubscribe` receives the sessions assigned to it and cancel requests, `report` stores its progress on the session, and `presentFrame` shows one of its screens in a host frame. Sessions end as `Delivered` or `Released` from the claims of the top-ups, or the completion of the payment request, the provider names. Hosts assign a session with `select_funding_provider` and implement `present_provider_frame`.
