---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`ChatResource::Hop { identifier, ticket, node, size }` names a file in the HOP pool of the person-to-person chat, so the chat product shows the photos and videos of native contacts. It is valid as media only, with a 32-byte identifier and ticket, a `wss://` node and a size up to 128 MiB. Native hosts open it with `openHopChatResource`, which claims it only from a node that `PersonChatPlatform` lists and returns the bytes with the pool entries, and ack it with `ackHopChatResource` once they keep the bytes. Hosts that match on `ChatResource` handle the new variant.

A room of the chat product that declares `hop_attachments` (in `ChatRoomFeatures` and the capability list) sends user photos, videos and files the way the native chat does: the host passes the file to `NativeTrUApiHostRuntime.chat_hop_attachment`, the core uploads it to a HOP node under a new ticket with the product's Bulletin allowance account and returns the `ChatResource::Hop` the host gives the product. The core records each upload under `CoreStorageKey::HopUpload` and never acks it, so a sender showing its own photo cannot drop it from the node before the contact claims it.

`decode_chat_push(key, message)` gives the text or call offer of a person-chat push from its route key, so a host shows it while no chat worker runs.
