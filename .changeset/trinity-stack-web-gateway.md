---
"@parity/trinity-stack": minor
---

Descriptors can name a `web-gateway`, the gateway that serves the sites of the stack's dotNS names, and an
`evm-explorer`, an Ethereum block explorer for Asset Hub. A `web-gateway` URL may carry a query, such as
`https://testnet.li/?network=previewnet`. A name's site is at that URL with its label prepended to the host, or, for a
name of several labels, with the name appended to its path, the query kept either way. `webUrl(descriptor, name)` builds that URL.
