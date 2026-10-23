---
"@parity/truapi": patch
---

On native hosts, a preimage lookup that misses keeps calling the app's `lookup_preimage` until it returns the value, so a lookup whose first fetch failed still resolves without the app calling `notifyPreimageChanged`. Retries start 6 seconds apart and back off to 30 seconds. They stop when the app returns the value or an error, or when the product unsubscribes. Keys that are not 32 bytes are not retried. Retries log `truapi.native.preimage.retrying` and `truapi.native.preimage.found`.
