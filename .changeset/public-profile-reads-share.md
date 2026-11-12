---
"@parity/truapi": patch
---

Public profile reads hold a bounded number of Asset Hub follows. Reads of one username that overlap share one read and one follow, and reads of different usernames run two at a time, so a screen of avatars resolving at once holds at most two of a connection's four follows.
