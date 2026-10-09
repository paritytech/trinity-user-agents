---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Serve `scanner.scan`. A host supplies the optional `scanner` callbacks to draw its viewfinder, and the core checks each request and each answer. The mock test host serves a scanner when created with a `scanner` answer, and `setScanAnswer` changes the next one. `contacts.pick` now stops waiting when the product cancels.
