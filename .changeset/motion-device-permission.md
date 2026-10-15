---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add the `Motion` device permission. Products request it through `requestDevicePermission("Motion")` or the standard `DeviceMotionEvent.requestPermission()`, and the iOS host answers WebKit's motion request from the product's saved, one-use, or prompted decision. A host built before this release cannot decode `Motion` and rejects the request with a malformed-frame error, so products should treat that error as motion being unsupported.
