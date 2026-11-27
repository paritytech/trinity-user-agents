---
"@parity/truapi": patch
---

`truapi-host` kept the auth session, device identity and signing keys in files that any user on the machine could read.
On Unix, state files under the base path are now written `0600` and the directories the CLI creates are `0700`. The core
database and the lock files keep the umask's permissions, but they sit inside those directories.

An existing install isn't changed in place. A file the CLI rewrites becomes `0600` on its next write, but
`device-storage.json` is written only once, and directories that already exist keep their mode. Run
`chmod -R go-rwx ~/.local/state/truapi-host`, or the same on the `--base-path` in use, to close them.
