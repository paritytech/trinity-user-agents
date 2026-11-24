---
"@parity/truapi": minor
---

`truapi-host` keeps the published descriptor a built-in network runs on in `<base-path>/<version>/stacks/<network>.json`
for the host commands (`pairing-host`, `signing-host`, `dev`). While the published descriptor cannot be fetched, and
when it is refused or older than that copy, the host starts from it instead of the bundled copy, whose chains are gone
after a wipe. A kept copy older than the bundled one is not used.
