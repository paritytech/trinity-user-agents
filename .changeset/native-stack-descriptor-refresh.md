---
"@parity/truapi-host": minor
---

Native hosts keep the Trinity stack descriptor current while the runtime runs. A host installs `NativeStackCallbacks`
with `set_stack_callbacks`: `fetch_stack_descriptor` makes the HTTP request and `stack_changed` receives a `StackStatus`
at start and on each change. `stack_status()` returns the current status and `refresh_stack()` asks for a fetch now. The
core keeps the last copy it verified in its database and starts from it when it is newer than the bundled one. Without
the callbacks the core stays on the copy it started from.
