---
"@parity/truapi": minor
---

Funding sessions log telemetry facts as exported `truapi.funding.*` markers: the overlay opening and closing, quotes settling, the provider chosen and taking the session, milestones, warning states, the ending, cancels and provider screens. Each detail reads `funding.ref=<ref> key=value ...`, with a random per-session reference and only the keys the core's registry lists for the marker, every value from a closed set: amounts and fees as buckets, a country as its region, never an intent, id, address, exact amount, provider text or URL. An ending that is a fault logs as `truapi.funding.ended_unexpectedly` at error. Funding's warnings and errors log as registered `truapi.funding.*` markers too, none naming the session.
