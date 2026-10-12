---
"@parity/truapi-host": patch
---

Pairing and session secrets are zeroized when dropped and redacted from debug output. Statement Store, Bulletin and PGAS allowance allocation sign personhood operations through a `PersonhoodSigner`, so callers pass a signer instead of raw entropy.
