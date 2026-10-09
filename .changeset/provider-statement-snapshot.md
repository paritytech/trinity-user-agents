---
"@parity/truapi-provider": patch
---

A `statement_subscribeStatement` on the light client starts with the statements its peers already store, ending with `remaining: 0`, as a full node's does. smoldot keeps no statement store and sent an empty first page at once, so a caller reading the initial pages, such as a Media lookup, found nothing while the peers replayed the matching statements a moment later as live notifications. The connection now collects that replay for the new subscription, for at most three seconds and until it has been quiet for half a second, and sends it as the initial pages.
