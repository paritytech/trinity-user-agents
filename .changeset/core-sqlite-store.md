---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Give native signing hosts a core-owned SQLite database. `HostRuntimeConfig.database_directory` (`databaseDirectory` in Swift and Kotlin) is required and names an existing, writable directory kept out of backups. The runtime opens `core.sqlite3` there at startup and refuses to start when it cannot. `coreDatabaseStatus()` reports the SQLite version, schema version and path.
