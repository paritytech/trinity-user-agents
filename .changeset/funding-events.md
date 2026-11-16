---
"@parity/truapi": major
---

The core reports funding facts for telemetry as `truapi.funding.*` events from a registry of allowed names and tags, and funding's warnings leave as registered `truapi.funding.*` markers without the session's intent.

Breaking for native hosts:

- `NativeFundingCallbacks` gains `funding_event(event)`. A `FundingEvent` carries its name, a level, a random
  `session_ref`, closed-value tags and numeric measures: amounts and fees as buckets, a country as its region, never an
  intent, id, address, exact amount or URL. Forward it only from builds that are not released.
- `FundingPlatform` gains `funding_event(event)`, which does nothing by default.
