---
"@parity/truapi": minor
---

The CLI host shows each navigation the core accepted in the terminal, and with `TRUAPI_NAVIGATIONS_LOG` set it appends one JSON line (`{"url", "at"}`) per navigation to that file before `navigate_to` returns, so a script driving a product can see where it navigated.
