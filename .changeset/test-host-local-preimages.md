---
"@parity/truapi-host": patch
---

A test host can keep preimage submissions in the core instead of sending them to the Bulletin chain, where a `store` signed with an allowance that was never authorized on chain is always refused at dry-run. Pairing and signing runtimes built with `test-host` take `setSubmitPreimagesLocally(true)`, and a signing host that grants allowances unchecked (`setGrantAllowancesUnchecked`) does it without being asked. The product gets the content key back and reads the value through the same lookup, from a store the core keeps for the rest of the run, and a refused `BulletinAllowance` still refuses the submission. Real hosts, which never set either, are unchanged.
