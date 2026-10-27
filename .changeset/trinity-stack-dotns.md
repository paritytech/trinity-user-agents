---
"@parity/trinity-stack": minor
---

`resolveDotns(pointer, descriptor, { store })` from `@parity/trinity-stack/dotns` reads a stack's dotNS contracts from
its Asset Hub, through the gateway's dispatcher and the protocol registry, and keeps them per `generation` for
`maxAgeMs`. It returns undefined for a stack whose Asset Hub has no dotNS gateway.
