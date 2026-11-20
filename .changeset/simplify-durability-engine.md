---
"@parity/truapi-host": patch
---

Simplify the native durability engine: each domain is registered with its chain, oracle and one policy, a recovery pass asks each oracle once without the domain's history, the builder alone starts and stops one worker per domain, each transaction cools down on its own before a rebuild, groups only label transactions for listing, and a transaction has at most one follower. Transactions of an unregistered domain stay as they are instead of being abandoned. Deferred (formerly scheduled) transactions keep their status names. The JavaScript API and wire protocol are unchanged.
