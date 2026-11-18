---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`notifications.sendPushNotification` takes an optional `title`, shown in place of the product id. Only a trusted product may set one. The core drops a blank title and the title any other product sends, and the call still succeeds. The client sends a request without a title as v0.1, so hosts that predate v0.2 keep serving it; a titled request needs a host that serves v0.2. A domain error comes back tagged `V2` whichever version was sent. `chat.postMessage` does the same for a message without `alt`.

Hosts receive the title as `HostPushNotificationRequest.title`, which is absent unless the core let it through, and show the product id when it is absent. The iOS and Android apps both title every product notification this way, so Android notifications now carry the product id as their title. The mock host records it as `NotificationLogEntry.title`, and the CLI prints it above the notification text.
