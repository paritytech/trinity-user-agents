---
"@parity/truapi-provider": minor
"@parity/truapi": patch
---

truapi-provider bundles the `paseo` network: the Paseo relay with Asset Hub Paseo, People Paseo
and Bulletin Paseo. A host starts any of them on the light client from its genesis hash alone, or
registers all four with `addNetwork("paseo")`. The statement store runs on People Paseo. The Paseo
relay is shared with `paseo-next-v2`, and a lookup by the relay's genesis hash still resolves to
`paseo-next-v2`; each parachain resolves to its own network. truapi-host compiles the catalog in,
so it ships the new specs too.
