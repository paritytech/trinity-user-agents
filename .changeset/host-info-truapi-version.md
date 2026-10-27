---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`system.info()` reports `truapiVersion`, the TrUAPI release whose runtime the host embeds, so a product can tag telemetry with it. The runtime fills it in on every host, including iOS and Android. A product built before this release ignores the field. A product built with it cannot decode the answer of an older host, so it should treat that error as the version being unknown.
