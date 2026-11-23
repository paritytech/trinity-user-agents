---
"@parity/truapi": major
"@parity/truapi-host": major
---

Remove the guest methods `worker.beginOperation` and `worker.endOperation`. Products should remove these calls. The host feature that needs a worker owns its lifetime. An assigned funding session already holds its provider through an outstanding payout or refund.

Calls from older guests to retired wire methods `(19, 0)` and `(19, 1)` receive `UnsupportedMessage`. Hosts can remove their product-operation callbacks. Disposing a browser or Node host ends its session immediately.
