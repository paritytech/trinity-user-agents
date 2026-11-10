---
"@parity/truapi": patch
---

dotNS reads for the same thing that overlap share one read and one Asset Hub follow: root and Worker manifest lookups of one product, and username lookups of one session, such as a host activating the same session twice in a row. A burst of them no longer fills a connection's follow budget. Nothing is kept once a read settles, so a failed read is retried by the next caller.
