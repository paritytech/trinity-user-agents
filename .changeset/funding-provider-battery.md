---
"@parity/truapi": patch
---

`make e2e-funding-cli` also runs a provider worker: the scripted funding host hands `provide` sessions to the product that asked, completes the top-ups and payment requests it starts, and the battery resumes an inbound session after a host restart.
