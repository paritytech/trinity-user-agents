---
"@parity/truapi": minor
---

Funding sessions log telemetry facts as exported `truapi.funding.*` markers: the overlay opening and closing, quotes settling, the provider chosen and taking the session, milestones, warning states, the ending, cancels and provider screens. Each detail reads `funding.ref=<ref> key=value ...`, with a random per-session reference and closed values only: amounts and fees as buckets, a country as its region, never an intent, id, address, exact amount, provider text or URL. An ending that is a fault logs as `truapi.funding.ended_unexpectedly` at error. Funding's warnings no longer name the session's intent or a quote id.
