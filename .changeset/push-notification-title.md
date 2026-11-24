---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`notifications.sendPushNotification` takes an optional `title`, shown in place of the product id. Only a trusted product may set one. The core trims whitespace and zero width characters from the title, drops one that is then empty and the title any other product sends, and the call still succeeds. The client sends a request without a title as v0.1, so hosts that predate v0.2 keep serving it; a titled request needs a host that serves v0.2. A domain error comes back tagged `V2` whichever version was sent.

Hosts receive the title as `HostPushNotificationRequest.title`, which is absent unless the core let it through. The app hosts show the product id when it is absent. The mock host records it as `NotificationLogEntry.title`, and the CLI prints it above the notification text.
