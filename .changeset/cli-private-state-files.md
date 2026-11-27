---
"@parity/truapi": patch
---

`truapi-host` writes its state readable by its owner only on Unix. `core-storage.json` and `device-storage.json`, which
hold the auth session, the device identity, the allowance and auto-signing keys and the device encryption key, were
written with the umask's permissions, usually `0644`, so another user on the machine could read them. They are now
written `0600`, like the account store, and so are product storage, session metadata, paired hosts, the current-user and
current-session pointers, the saved log level and kept light specs. The directories the CLI creates for its state are
`0700`. A file an earlier version left readable becomes owner-only on its next write. Directories that already exist
keep their permissions, so an existing state directory stays as readable as it was until it is changed by hand, for
example with `chmod -R go-rwx ~/.local/state/truapi-host`.

Every state file is replaced the same way: a new temporary file beside it, synced, renamed over it, and the directory
synced. `stack publish` and `stack bundle` output still takes the umask's permissions, so a web server can serve it.
