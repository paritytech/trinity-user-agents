---
"@parity/truapi-host": major
---

A host build can add to the core's reserved values. `stack.reservedValues` replaces `stack.allowsReservedValues`, and
the test host's `testStack.reservedValues` replaces `testStack.allowsReservedValues`. Both take a `ReservedValuesPolicy`,
whose doc says what the core reserves and which policies it refuses. The core refuses a field `stack` or `testStack`
does not have, so a config that still passes `allowsReservedValues` is refused by name.
