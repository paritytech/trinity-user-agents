---
"@parity/truapi": patch
---

On native hosts, a preimage lookup that misses keeps calling the app's `lookup_preimage` until it returns the value, so a lookup whose first fetch failed still resolves without the app calling `notifyPreimageChanged`. Retries run every 6 seconds for the first 30 seconds, then every 30 seconds. They stop when the app returns the value or an error, or when the product unsubscribes. On every host, a key that is not 32 bytes gets a miss without the host being asked. Retries log `truapi.native.preimage.retrying` and `truapi.native.preimage.found`.
