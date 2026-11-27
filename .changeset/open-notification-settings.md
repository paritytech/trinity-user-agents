---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`system.navigateTo` accepts `polkadotapp://settings/notifications` and opens the notification settings the operating system keeps for the host application on iOS and Android. The link needs no `OpenUrl` grant, and `app-settings:` is rejected.

`parse_navigate` classifies the link as the new `NavigateDecision::NotificationSettings`, so a host that matches `NavigateDecision` exhaustively needs an arm for it. The core hands the link to the host through `navigateTo` and exports it as `notification_settings_url()`. A host that does not compare against it receives a `polkadotapp:` link no grant was asked for.
