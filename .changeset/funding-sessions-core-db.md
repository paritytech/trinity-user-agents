---
"@parity/truapi": major
---

Funding sessions are stored in the core database, one row per session, so funding needs one: natively the configured `database_directory`, otherwise `set_core_db`. The `FundingSessions`, `FundingSessionPack` and `FundingSessionsUnreadable` core storage keys are gone.
