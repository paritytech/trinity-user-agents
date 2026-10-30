---
"@parity/truapi": major
"@parity/truapi-host": minor
---

`createTransaction` moves to v0.2 and adds a `ChainUnavailable { reason }` error for a chain that stays unreachable, such as during a halt. Nothing was signed, so a product can retry on it, with a bound on the attempts. v0.1 callers still get `Unknown`. A product built against this version needs a host that serves v0.2. Older hosts answer with `MalformedFrame`.
