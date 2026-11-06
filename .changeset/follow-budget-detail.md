---
"@parity/truapi": patch
---

The `truapi.chain.follow_budget_reached` line now says what each follow holding a slot is doing: how long it has been open, when the node last sent it an event, how many chainHead calls were made on it and when the last one was, and whether its receiver is still open.
