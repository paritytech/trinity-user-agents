---
"@parity/truapi": patch
---

When `truapi-host` cannot open a session's core database, for example because a newer `truapi-host` migrated it, the signing host stops with the `rm -f` command that deletes it. Running that command starts the session clean and keeps its accounts and pairings.
