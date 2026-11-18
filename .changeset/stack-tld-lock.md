---
"@parity/truapi-host": minor
---

A stack keeps the TLD of the descriptor its build bundles. The core refuses a fetched copy that names another TLD:
`lastRefresh` is `refused`, the core keeps the copy it holds, and `restartRequired` is never set for such a copy. A
stored copy on another TLD is ignored. The core starts from the bundled copy, and the next copy it takes replaces the
stored one. Accounts derive under the TLD, so only a new build changes it.
