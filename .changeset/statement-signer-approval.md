---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A statement proof signed with another product's account asks the user once per account for the life of the product
execution, instead of once per statement. A refusal is not remembered, so the next proof asks again. Proofs requested
while that prompt is open wait for its answer rather than opening their own.
