---
"@parity/truapi": patch
---

A json-rpc request the host accepts and never answers fails after 10 seconds instead of parking its caller forever. The budget covers the response frame, which carries a call's return value or a subscription's id, and not the subscription's events, which take as long as a cold light client needs. A host whose socket dies with a chainHead follow queued therefore ends that follow, so the core drops the chain client built over it and the next transaction build constructs a fresh one, where before every build on that connection failed until the app was restarted.
