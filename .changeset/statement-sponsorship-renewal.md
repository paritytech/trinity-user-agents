---
"@parity/truapi-host": minor
---

A kept Statement Store key is reused across allowance periods instead of being looked up again each UTC day, and a statement the store rejects no longer discards it. Before each submit signed with a kept key, a signing host renews that key's sponsorship, so every such submit first runs the renewal check against the chain: it opens a chain client, reads the network suffix, takes the renewal registration lock and scans the personhood collections, and registers the key again when the period changed. A paired host sends nothing, because the phone renews the sponsorships it issued.
