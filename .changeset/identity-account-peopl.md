---
"@parity/truapi": major
---

The identity account is the personhood product's account 0, `//product//peopl.<tld>/index_bytes(0)` (RFC-0028), and owns the person's lite and membership usernames. The `uid.<tld>` reserved product is retired. The same seed therefore derives a different identity account than before: local activation, pairing attestation, the SSO statement key, username lookup and the CLI's `attest`, `register-name` and `identity-check` all use the new account, and a name reserved under `uid.<tld>` does not resolve for it. Hosts that derive the main account natively (iOS, Android) move with it.
