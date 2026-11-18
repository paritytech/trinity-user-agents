---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`system.navigateTo` accepts `polkadotapp://settings/notifications`, which opens the notification settings the operating system keeps for the host application on iOS and Android without an `OpenUrl` grant. A host built before this release treats the link as an external app link, asks for `OpenUrl` and opens nothing. `app-settings:` stays rejected.
