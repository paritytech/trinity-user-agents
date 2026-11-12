---
"@parity/truapi": patch
---

Public profile reads share one Asset Hub follow. Reads asked for at once queue for a single task that, once per round, opens a follow, finds the profile resolver and reads up to 64 queued usernames' records side by side, so a screen of avatars resolving at once holds one of a connection's four follows and resolves in about the time of one profile.
