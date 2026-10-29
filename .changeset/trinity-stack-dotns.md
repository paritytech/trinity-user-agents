---
"@parity/trinity-stack": minor
"@parity/truapi": minor
---

A descriptor can state its stack's dotNS contract addresses in `dotns`, by protocol registry key, and
`truapi-host stack` and `@parity/trinity-stack` validate it. `npx trinity-stack add` and `update` write a project's
signed stack copies to `.trinity/` with a module that bundles them. `createStackReader` from
`@parity/trinity-stack/reader` runs from those copies and refreshes them by a `"never"`, `"once"` or `{ maxAgeMs }`
policy, with `If-None-Match`, `onChange` and `require`. `npx trinity-stack check` and `checkDotns` compare a
descriptor's `dotns` with the registry at Asset Hub's latest finalized block.
