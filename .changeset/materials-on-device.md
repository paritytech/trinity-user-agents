---
"@parity/truapi-host": minor
---

Native hosts keep materials in the core database. The core reads a material from Bulletin itself, over its own Bulletin connection, and keeps only text that hashes to its ID. Materials a saved Pocket face draws are never evicted, and the rest share 4 MiB. At start-up the core fetches the materials saved faces draw and the database lacks. Hosts change nothing.
