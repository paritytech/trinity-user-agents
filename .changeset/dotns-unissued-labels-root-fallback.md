---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A session whose identity account holds only dotNS labels the PoP controller did not issue, such as a publicly registered name, resolves its username from the root key. Those labels are not usernames, so they no longer end the lookup with an empty result.
