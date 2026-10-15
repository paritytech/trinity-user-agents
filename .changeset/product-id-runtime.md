---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A signing host derives a paired product's AutoSigning ring-VRF entropy and its statement-store and Bulletin allowance
accounts from the normalized product id, so `MyApp.dot` gets the same keys as `myapp.dot`. A request from a paired host
whose calling product id is not a product id is refused before anything is derived or the user is asked.
