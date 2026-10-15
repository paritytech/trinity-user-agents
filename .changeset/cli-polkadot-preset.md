---
"@parity/truapi": patch
---

`truapi-host` gains a `polkadot` network preset for Polkadot mainnet: Polkadot People,
Bulletin and Asset Hub, dotNS under `.dot`, and the `DotnsPopController` from the dotNS
Polkadot deployment manifest as a fallback while the gateway dispatcher is unset. It ships
no identity backend; set `HOST_CLI_IDENTITY_BACKEND_BASE` for commands that need one.

The plaintext account store now holds mnemonics only for presets whose identities are
disposable. On `polkadot` the CLI signs only with `--mnemonic` or `HOST_CLI_SIGNER_MNEMONIC`,
and refuses auto accounts, stored accounts and mnemonic imports. It also refuses automatic
approval there, so `dev`, `--auto-accept` and `/approval automatic` cannot sign for a real
identity without a prompt. The two test presets behave as before.
