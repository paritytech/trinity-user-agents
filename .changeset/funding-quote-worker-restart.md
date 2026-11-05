---
"@parity/truapi": patch
---

A quote asked of a provider whose worker the host is stopping and starting again waits, within the quote's deadline,
for the new worker's connection instead of failing on the old one, which keeps its link until its connection closes.
