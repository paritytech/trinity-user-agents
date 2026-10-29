---
"@parity/truapi": major
"@parity/truapi-host": minor
---

`createTransaction` moves to v0.2, whose error adds `ChainUnavailable { reason }`: the chain the call depends on stayed unreachable, so nothing was built, sent or signed. A host building the transaction itself returns it when the node serving the chain could not be asked for its current state, as while a halted chain is redialed, or gave no answer within 45 seconds. A host relaying the request to the signing host returns it when the request never reaches the signing host, because the chain it travels over stays halted or reaches no peer. A product can retry on that variant alone instead of matching the `Unknown` reason, with a bound on the attempts. A chain the host cannot connect to, a provider that refuses to follow the chain for a reason other than a halt, a runtime the host cannot use, and a relayed request that failed after it reached a peer still fail as `Unknown`. A v0.1 caller receives `Unknown` with the same reason. `createTransactionWithLegacyAccount` stays on v0.1. A product built against this version needs a host that serves v0.2 of `createTransaction`, since an older host answers it with `MalformedFrame`.
