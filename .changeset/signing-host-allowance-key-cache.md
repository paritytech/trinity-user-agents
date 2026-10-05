---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

The local signing host caches a product's statement-store allowance key for the current allowance period, including a key allocated by an explicit resource allocation request, so proofs after it in a period skip the on-chain slot scan. The key is looked up again when the period changes, the local session is cleared or replaced, the product's state is cleared, or the statement store still rejects a statement signed with it for having no allowance after the submit retries. Statement submissions retry a `noAllowance` rejection up to 10 times, 2 seconds apart, as the native iOS and Android hosts do.
