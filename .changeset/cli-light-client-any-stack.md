---
"@parity/truapi": patch
---

`truapi-host --chain-backend light-client` runs every built-in network, including `pcf-devnet`, and any `--stack`. It
takes the light specs the stack's descriptor names and checks them against the descriptor. The binary ships the specs
of the built-in networks, so they start without the spec servers; other specs are fetched once and kept under the state
directory. A stack whose descriptor names no `lightSpec` for the relay, People, Asset Hub or Bulletin is refused at
start, and so is one whose relay spec cannot be fetched. `truapi-host stack bundle --light-specs <file>` writes the
light specs a descriptor names, checked against it.
