---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`system.info()` reports `truapiVersion`, the TrUAPI release whose runtime the host embeds, so a product can tag telemetry with it. The runtime fills it in on every host, including iOS and Android. A host built before this release cannot decode the v0.2 request and rejects it with a malformed-frame error, so products should treat that error as the version being unknown.
