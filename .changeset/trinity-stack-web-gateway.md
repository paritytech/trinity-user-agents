---
"@parity/trinity-stack": minor
"@parity/truapi": minor
---

Descriptors can name a `web-gateway`, the gateway that serves the sites of the stack's dotNS names, and an
`evm-explorer`, an Ethereum block explorer for Asset Hub. `webUrl(descriptor, name)` gives the URL a name opens at on
the stack's gateway. A `web-gateway` may use plain `http` on `localhost`, for a stack run on the reader's machine.
