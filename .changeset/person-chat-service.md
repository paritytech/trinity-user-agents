---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`personChat.getProfile`, `personChat.chatSecrets`, `personChat.proveStatement` and `personChat.proveChatRequest` give the person-to-person chat product (the `chat` label on any dotNS network) what it needs to run the chat protocol of the iOS and Android apps: the identity's chat profile, the X25519 secrets of the identity chat key and the device key, statement proofs by the identity account, and identity signatures on chat requests. Every other product gets `Unsupported`. A signing host serves all four; a pairing host proves statements with its device statement key and answers `Unsupported` to `proveChatRequest`.

`personChat.pushTokens` gives the chat product the push and VoIP tokens of the device, and `personChat.notifyPeer` relays a chat push, already encrypted for the contact, through the host's push service in the native chat's shape (a 64-character push id, a hex message, and the VoIP flag for call offers). `PushPlatform` gains `push_tokens`, and `HostPushDelivery` gains `voip`; hosts that implement `PushPlatform` implement `push_tokens` too.

`personChat.hopUpload`, `personChat.hopDownload` and `personChat.hopAck` carry a file of up to 4 MiB (a compaction batch) through a HOP node in the native chat's format: the core seals and chunks it under a fresh ticket, signs the submit with the product's Bulletin allowance account, downloads from the pool or from Bulletin once an entry was promoted, and acks the entries it took. `PersonChatPlatform` (`set_person_chat_platform`, `NativePersonChatCallbacks` on native hosts) gives the HOP nodes the host trusts; without it, and in the browser core, the calls answer `Unsupported`.

`personChat.prepareDelivery` and `personChat.proveDelivery` deliver a chat request as the native apps do: the core derives the request's delivery account (`//chat-request//<request id>//<period>` from the root), claims a People notification slot for it with `Resources.set_notification_statement_account_for_sequence` under a ring-VRF `RegisterNotificationForCollection` origin, waits up to five minutes for the statement allowance, and proves the request statement with that account. `NoSlot` means no slot is free, and the request goes out under the identity.

`personChat.publishContacts` gives the host the chat's contacts (account, username, room, blocked) for the parts of the app outside the chat. `PersonChatPlatform` gains `paired_devices` and `set_contacts` (`NativePersonChatCallbacks` too), so hosts that implement it implement both; `getProfile` lists the paired devices in `ownDevices`.

`personChat.historyExport` and `personChat.historyImported` let the chat product import the native chat history an app staged when it moved the install's native chat over, in parts of at most 1 MiB; `PersonChatPlatform` gains `history_export` and `history_imported`. The native runtime gains `import_chat_device_key`, which keeps the native chat's device key in the new `ChatDeviceKey` core storage slot; the chat product's device key is that key when present, and the device encryption key SSO pairing advertises is unchanged.
