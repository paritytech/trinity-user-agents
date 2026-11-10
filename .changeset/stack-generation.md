---
"@parity/truapi-host": major
---

A cached product or Worker manifest records the stack's `generation` and is not read under another one. After a wipe
the core reads manifests again from the new Asset Hub and keeps the session and stored permissions. A manifest cached
before this change is read again once.
