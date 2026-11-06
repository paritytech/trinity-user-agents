---
"@parity/truapi": major
---

`truapi-host --network pcf-devnet` runs against PCF's devnet on Paseo, read from the descriptor PCF signs. It is a test network like `paseo-next-v2` and `previewnet`, on Polkadot's `dot` TLD.

`--network polkadot` is removed, and with it the restrictions it carried: every network now allows the account store, auto-accept, `dev`, `--product-config`, `attest` and a local-network file. Forks of Polkadot run, with Polkadot's genesis hashes and the `dot` TLD. An endpoint with one of Polkadot's genesis hashes is compared with real Polkadot, whose signed descriptor the release ships, by block hash at the lower of the two finalized blocks, and the command stops when it serves Polkadot itself or when it, or every Polkadot node, does not answer. A `--stack` with the id `polkadot` is refused, and a `polkadot-fork` stack runs under its own namespace.
