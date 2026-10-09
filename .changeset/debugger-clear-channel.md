---
"@parity/truapi-debugger": patch
---

Add a per-channel clear to the standalone board.

`POST /clear?channel=<id>` drops the operations recorded for one channel, and the toolbar gains a `clear` button that
acts on the selected channel. A board can serve several hosts at once, so clearing the one being worked on leaves the
others alone; the route refuses an unscoped clear rather than treating it as "all". It is the only route that mutates,
so it takes the same Origin gate as the WebSocket upgrade.
