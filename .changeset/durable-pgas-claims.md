---
"@parity/truapi-host": patch
---

Native signing hosts claim PGAS for a `SmartContractAllowance` through the durable ledger. A destination has at most one claim under way, so a retried or concurrent request waits on it instead of spending another slot, and a claim is built only while the account that asked for it is signed in. Requests are answered as before, once the claim is in a best block, and answer `NotAvailable` after three minutes or when cancelled while the claim carries on. The wasm test host keeps claiming directly. The JavaScript API and wire protocol are unchanged.
