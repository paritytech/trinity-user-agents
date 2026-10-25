---
"@parity/truapi-host": minor
---

Account access, operation reviews and cross-product signature approvals go through one `ProductConsent` per runtime. An Allow once answer to an account-access prompt now covers that product's later requests until restart, lock, a wallet switch or that product's reset, only one account-access prompt is shown at a time, and a request that waited for it reuses the answer. Requests from a paired host share the phone's stored decisions and are always reviewed on the phone.
