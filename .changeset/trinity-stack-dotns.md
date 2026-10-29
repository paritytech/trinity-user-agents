---
"@parity/trinity-stack": minor
---

`npx trinity-stack add` and `update` write a project's stack copies to `.trinity/`: each stack's signed descriptor, its
dotNS contracts and a module that bundles them. `createStackReader` from `@parity/trinity-stack/reader` runs from those
copies and refreshes them by a `"never"`, `"once"` or `{ maxAgeMs }` policy, with `If-None-Match`, `onChange` and
`require`. `readDotns` from `@parity/trinity-stack/dotns` reads a stack's dotNS contracts at Asset Hub's latest
finalized block.
