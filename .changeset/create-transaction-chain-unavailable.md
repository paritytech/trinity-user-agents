---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`createTransaction` moves to v0.2, whose error adds `ChainUnavailable { reason }`: the host builds the transaction itself and the node serving the chain could not be asked for its current state, as while a halted chain is redialed, or gave no answer within 45 seconds, so nothing was built or signed. A product can retry on that variant alone instead of matching the `Unknown` reason, with a bound on the attempts. A chain the host cannot connect to, a provider that cannot follow the chain, a runtime the host cannot use, and a transaction relayed to the signing host still fail as `Unknown`. A v0.1 caller receives `Unknown` with the same reason. `createTransactionWithLegacyAccount` stays on v0.1. A product built against this version needs a host that serves v0.2 of `createTransaction`, since an older host answers it with `MalformedFrame`.
