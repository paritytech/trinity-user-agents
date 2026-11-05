---
"@parity/truapi-host": minor
---

The test host takes two more of `@parity/host-api-test-sdk`'s spellings:

- `setPermissionBehavior("approve-all")` allows every permission, as `"allow-all"` does.
- `injectStatement` takes a `proof`, with `0x`-hex fields, and refuses one whose fields do not fit its kind.
