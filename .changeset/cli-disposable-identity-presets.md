---
"@parity/truapi": patch
---

`truapi-host` writes mnemonics to its plaintext account store only for network presets
whose identities are disposable. On any other preset it signs only with `--mnemonic` or
`HOST_CLI_SIGNER_MNEMONIC`, and refuses auto accounts, stored accounts and mnemonic imports.
It also refuses automatic approval there, so `dev`, `--auto-accept` and `/approval automatic`
cannot sign for a real identity without a prompt.
Both shipped presets are test networks, so their behaviour is unchanged.
