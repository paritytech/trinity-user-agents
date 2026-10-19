---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

`account.get_account` for a product's own account raises one Product Account confirmation and sends one subtree request to the paired signing host however many calls are waiting, and every waiting call takes that outcome. A call made after it settles asks again, so a rejection or a failed fetch is not remembered. A cancelled call leaves the others waiting.
