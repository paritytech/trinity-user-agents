---
"@parity/truapi": minor
---

Funding sessions emit telemetry facts as core events under exported `truapi.funding.*` markers: the overlay opening and closing, quotes settling, the provider chosen and taking the session, milestones, warning states, the ending, cancels and provider screens. Each carries typed attributes, a random per-session `funding.ref` and closed values only: labels, conditions and counts, amounts and fees as buckets, a country as its region, never an intent, id, address, exact amount, provider text or URL. An ending that is a fault goes out as `truapi.funding.ended_unexpectedly` at error. Funding's warnings no longer name the session's intent or a quote id.
