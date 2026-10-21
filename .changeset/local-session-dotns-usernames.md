---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

A wallet-local session reads its usernames from the dotNS contracts on Asset Hub, the same lookup a paired session uses. Activation does not wait for it: the session starts under the lite username the host passes, if any, and when the dotNS record holds a name, the record's lite and full usernames replace it and the core reports them through `authStateChanged`. Hosts no longer need to read usernames from the People chain.
