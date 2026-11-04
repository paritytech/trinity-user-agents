---
"@parity/truapi": patch
---

A chain-head call on a follow that has ended now names why it ended instead of reporting `unknown follow subscription id`: `ended when its connection closed`, `was stopped by the node`, or `ended on an error with its connection still open`. Only the reason text changes, not the error kind. A spent id is also no longer answered by an unrelated follow.
