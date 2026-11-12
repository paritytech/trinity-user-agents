---
"@parity/truapi": patch
---

Public profile reads hold a bounded number of Asset Hub follows. Reads of one username that overlap share one read and one follow, and reads of different usernames run two at a time, so a screen of avatars resolving at once leaves the rest of a connection's follow budget to everything else on it.
