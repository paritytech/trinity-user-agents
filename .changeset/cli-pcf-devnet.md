---
"@parity/truapi": major
---

`truapi-host --network pcf-devnet` runs against PCF's devnet on Paseo, read from the descriptor PCF signs. It is a test network like `paseo-next-v2` and `previewnet`, and the only network that may use Polkadot's `dot` TLD.

`--network polkadot` is removed, and with it the restrictions it carried: every network now allows the account store, auto-accept, `dev`, `--product-config`, `attest` and a local-network file. No network may route to Polkadot's chains, and a `--stack` that uses the `dot` TLD or runs under the `polkadot` or `pcf-devnet` namespace is refused.
