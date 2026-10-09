---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Statement-store allowance lookups read a period's slot row in one request per collection, and read the People and LitePeople rows at the same time, instead of one request per slot.
