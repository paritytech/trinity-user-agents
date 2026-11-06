---
"@parity/truapi": major
---

`truapi-host --network pcf-devnet` runs against PCF's devnet on Paseo, read from the descriptor PCF signs, with Polkadot's identity protections: its `dot` TLD derives the same keys as Polkadot, so it signs only with `--mnemonic`, never stores an account, never auto-approves, and refuses `--product-config` and `attest`. A published pcf-devnet descriptor that changes a genesis hash, the TLD, `assets`, `contracts`, `dotns` or `coinage` is not used, and `truapi-host stack bundle` refuses such a copy for its link.

`--network polkadot` is removed. No network may route to Polkadot's chains, and a `--stack`, a `HOST_CLI_*_WS` override or a local-network file that uses the `dot` TLD, or a stack under the `polkadot` or `pcf-devnet` namespace, is refused.
