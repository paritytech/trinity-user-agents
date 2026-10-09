---
"@parity/truapi-host": minor
---

The test host's statement store reads the field vector the core sends, so a
topic filter matches what a product submitted, and a suite reads statements
back as decoded entries. `behaviors.resourceAllocation` withholds a named
resource across every way a product reaches it, including the allowance keys
the statement-proof and preimage paths ask for without requesting an
allocation. A changed permission answer reaches the core, and product storage
is readable under the name `@parity/host-api-test-sdk` gives it.

Surface a migrating suite has to match:

- `PermissionLogEntry` carries `decision` and `timestamp` as required fields,
  so an assertion comparing a whole entry names both.
- `injectStatement` answers the `StatementEntry` the store retained, and takes
  `{topics, data}` as well as the SCALE wire bytes.
- `getSubmittedStatements` answers `StatementEntry[]`, so a read of a
  statement's payload goes through `entry.data` rather than the `0x` hex.
- The loopback statement store rides on the People chain's proxy, or on the
  single proxy of a one-chain suite. Two or more chains with no People among
  them carry no store: a suite that passed `loopbackStatements: true` is
  refused and told to declare the People chain or drop to one chain, and a
  suite that only took the default gets no store and builds.
