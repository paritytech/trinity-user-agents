---
"@parity/trinity-stack": minor
---

`npx trinity-stack add` and `update` write a project's signed stack copies to `.trinity/` with a module that bundles
them. `createStackReader` from `@parity/trinity-stack/reader` runs from those copies and refreshes them by a `"never"`,
`"once"` or `{ maxAgeMs }` policy, with `If-None-Match`, `onChange` and `require`.
