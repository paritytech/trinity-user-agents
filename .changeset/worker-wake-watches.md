---
"@parity/truapi": minor
---

Add `worker.setWakeWatches`. A worker names the statement-store topics whose new statements wake it, each group routed to one of its chat rooms, and the answer says whether a push relay serves them (`Push`) or not (`Unavailable` with a reason). Each call replaces the earlier watches; an empty list removes them. A wake holds the worker for 30 s and sends `ChatRoomEvent.Woken` to the room of the watch, at most 12 times in one hour. A host that does not know the method answers `unavailable`.
