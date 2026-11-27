---
"@parity/truapi": patch
---

`truapi-host` kept the auth session, device identity and signing keys in files that any user on the machine could read.
On Unix, its state files are now written `0600` and the directories it creates are `0700`.

An existing install isn't changed in place: `device-storage.json` and existing directories keep their permissions. Run
`chmod -R go-rwx ~/.local/state/truapi-host` to fix them.
