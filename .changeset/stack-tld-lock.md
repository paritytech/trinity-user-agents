---
"@parity/truapi-host": minor
---

A stack keeps the TLD of the descriptor its build bundles. The core refuses a fetched copy that names another TLD, with
`lastRefresh` set to `refused`, and keeps the copy it holds. A stored copy on another TLD is ignored, and the core
starts from the bundled copy. Accounts derive under the TLD, so only a new build changes it.
