---
"@parity/trinity-stack": minor
---

`npx trinity-stack add` and `update` write a project's signed stack copies to `.trinity/`, and `trinity-stack generate`
rebuilds the git-ignored module that bundles them without fetching. `createStackReader` from
`@parity/trinity-stack/reader` runs from those copies and refreshes them by a `"never"`, `"once"` or `{ maxAgeMs }`
policy, with `If-None-Match`, `onChange` and `require`.
