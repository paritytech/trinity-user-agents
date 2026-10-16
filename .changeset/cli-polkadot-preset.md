---
"@parity/truapi": minor
---

`truapi-host` gains a `polkadot` network preset for Polkadot mainnet: Polkadot People, Bulletin
and Asset Hub, with dotNS under `.dot`. Its identities are real, so on `polkadot` the CLI signs
only with `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC`, never stores an account, never approves a
confirmation without a prompt, and refuses `--product-config`. It ships no identity backend; set
`HOST_CLI_IDENTITY_BACKEND_BASE` for commands that need one. The test presets are unchanged.
