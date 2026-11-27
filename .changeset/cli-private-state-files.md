---
"@parity/truapi": patch
---

On Unix, a new `truapi-host` install keeps its state readable by its owner only. The directories the CLI creates under
the base path are `0700`. `core-storage.json` and `device-storage.json`, which hold the auth session, the device
identity, the allowance and auto-signing keys and the device encryption key, are written `0600`, like the account store,
and so are product storage, session metadata, paired hosts, the current-user and current-session pointers, the saved log
level and kept light specs. Each is replaced through a temporary file beside it that is created with that mode, synced
and renamed over it. The core database (`core.sqlite3` with its `-wal` and `-shm` files) and the lock files still get the
umask's permissions and sit inside those `0700` directories. `installation` is created once, `0600`.

On an existing install, a file the CLI rewrites becomes `0600` on its next write. A file written once, such as
`device-storage.json`, a file rarely rewritten, such as `session.json`, and the directories that already exist keep their
permissions until `chmod -R go-rwx ~/.local/state/truapi-host` is run, or the same on the `--base-path` in use.

`stack publish` and `stack bundle` output still takes the umask's permissions, so a web server can serve it.
