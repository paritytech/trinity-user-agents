---
"@parity/truapi": minor
---

`truapi-host` gains a `polkadot` network preset for Polkadot mainnet: Polkadot People, Bulletin
and Asset Hub, with dotNS under `.dot`. Its identities are real, so on `polkadot` the CLI signs
only with `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC`, never stores an account, never approves a
confirmation without a prompt, and refuses `--product-config` and `attest`. It ships no identity
backend; set `HOST_CLI_IDENTITY_BACKEND_BASE` for commands that need one.

An endpoint rerouted by `HOST_CLI_*_WS` or `HOST_CLI_LOCAL_NETWORK_CONFIG` must report the genesis
it is routed as, and a test preset refuses any route carrying a real-identity preset's genesis.
`--help` no longer prints an exported `HOST_CLI_SIGNER_MNEMONIC`.
