---
"@parity/truapi": patch
---

`truapi-host --chain-backend light-client` runs every built-in network, including `pcf-devnet`, and any `--stack`. It
fetches the light specs the stack's descriptor names, checks them against the descriptor, and keeps each checked spec
under its state directory. A stack whose descriptor names no `lightSpec` for the relay, People, Asset Hub or Bulletin
is refused at start.
