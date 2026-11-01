---
"@parity/truapi-host": minor
---

A web signing host can give a product's own account at a raw derivation index a general Statement Store allowance slot
of the session's personhood for the current UTC day. `claimProductStatementAccount(productId, index, submit)` on the web
worker runtime, over `claim_product_statement_account` on the Rust signing host runtime, derives the account from the
session root and the named product id, so only that product's accounts can be funded, and registers it in the first free
slot, People before LitePeople. With `submit` false it only asks the runtime whether it would accept the claim and
submits nothing. It reads every collection first: an account that already holds a slot this period in any of them claims
nothing again and needs no ring. A live slot is never replaced. It submits at most once: a failed submission is settled
by reading every collection again, and a target still unregistered is reported uncertain, never followed by a claim on
another slot. The claim stops using the session's personhood keys once the session ends. The account is not added to the
renewal ledger. Products sign with the account through `statementStore.createProof`; the host decides when a product may
have one. Signing hosts on the `testing` bundle only; the iOS and Android host runtimes do not expose it.
