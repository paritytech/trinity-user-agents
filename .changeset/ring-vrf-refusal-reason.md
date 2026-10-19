---
"@parity/truapi-host": patch
---

A ring-VRF proof, alias read or signature refused as `NotAllowlisted` is logged as a warning, so a native host receives it as a `truapi.tracing.warn` line. The line names the reason, the `trustedProducts` entry checked and the scopes it lists, and where the manifest came from: the cache and its age in seconds, a fresh dotNS read, or the lookup failure. The error a product receives is unchanged.
