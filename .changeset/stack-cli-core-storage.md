---
"@parity/truapi": minor
---

`truapi-host` keeps core state per stack, so a fork and its source can share a state directory and run at once.
`core-storage.json` keys are hex of a `StackStorageKey`, every change to the file holds a lock on
`core-storage.json.lock` beside it, and the pairing host remembers its last user per stack in
`pairing-host/current-user.<stack>`. State written by an earlier version is kept: the first stack to run takes it.

A downgrade reads none of the state written since, only what the earlier version left under the old keys.
