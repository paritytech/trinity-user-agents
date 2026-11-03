---
"@parity/truapi-host": patch
---

Contract reads and writes share `EvmContractCaller`. Rust callers prepare a contract transaction with its signer, then
execute the opaque request and wait for successful finalized dispatch. Revive wire types and resource estimates are
private; dotNS reads retain their transport's block context and distinguish reverts from failed requests.
