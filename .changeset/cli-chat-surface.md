---
"@parity/truapi": minor
---

`truapi-host dev` serves a product's chat worker beside its App. The worker
connects on `/worker` as a Worker of the same product and session, booted from a
bundle given by `--worker-bundle` by the page at `/chat`, where a person reads
what the worker posts, replies, presses action buttons and sends `/commands`,
which reach the worker through `chat.actionSubscribe()` with peer `native` as on
iOS. The page also shows the worker's own log beside the thread: its console,
captured ahead of the boot, levelled, filterable and following the tail unless
paused. Rooms, chat and log are each a pane to show, hide or resize, and the
page remembers the layout.

The in-memory chat host keeps rooms, bots and messages and reports every change
to the page over `/chat/ws`. Signing and pairing hosts are unchanged: the routes
exist only when `dev` runs.
