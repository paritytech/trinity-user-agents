---
"@parity/truapi": minor
---

With `TRUAPI_ACCOUNT_REQUESTS_DIR` set, `signing-host --serve` answers requests for the served product's account at a derivation index (public key and address) through files in that directory, so a script can learn which account the host signs with. Other products are refused.
