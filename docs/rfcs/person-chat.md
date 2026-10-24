---
title: "PersonChat host capability"
owner: "Leonardo Custodio"
status: draft
---

# RFC — PersonChat host capability

## Summary

`PersonChat` (trait `30`) gives the chat product the keys and host services of the person-to-person chat protocol, so
the chat runs in the product's Worker while the host draws it through [Chat Extension v2](chat-extension-v2.md). The
core serves it to the chat product only. The trait has sixteen methods, ids `0` to `15`.

## Motivation

The chat product (`chat.<tld>`) keeps the person-to-person conversations of the app and speaks the chat protocol with
contacts whose apps run the native chat. The APIs every product gets cannot speak that protocol:

- Contacts address the identity, not a product account. They encrypt to the identity's chat key and to each device's
  encryption key, and accept only statements signed by the identity account or by a device's statement account.
- A chat request goes out from an anonymous delivery account derived from the root entropy, which a notification slot of
  the user's personhood ring lets post. Only the host holds the root entropy and the ring membership.
- A push to a contact goes through the app's push relay with the app's credentials, and a push that arrives while no
  worker runs is shown by the host's notification receiver.
- The wallet, caller names, the contact picker of other products, the native chat's history and the coinage claims of
  payments are host data.

## Requirements

- **Compatibility**: a contact on the native chat sees the product as the same identity and devices, with the same keys,
  signatures and derivations.
- **Custody**: no sr25519 secret and no root entropy leaves the host.
- **Scope**: no product but the chat product reaches any method.
- **Bounds**: no method signs arbitrary bytes as the identity.
- **Background**: a contact's push shows while no worker runs.

## Approach

The design has six parts:

- Access: who is served and how a call fails.
- Identity: the profile, the chat secrets and the identity signatures.
- Delivery: the anonymous accounts of chat requests.
- Pushes: the tokens, the relay and the background routes.
- HOP: files a contact claims from the HOP pool.
- Host data: what the host keeps for the rest of the app.

### Access

The methods are defined in [`api/person_chat.rs`](../../rust/crates/truapi/src/api/person_chat.rs) and their payloads in
[`v01/person_chat.rs`](../../rust/crates/truapi/src/v01/person_chat.rs).

- The core serves the trait only in a Worker of the chat product, the `chat` label on any dotNS network. Any other
  product gets `Unsupported` before the call runs.
- No call needs a permission or asks the user.
- A host that cannot back a call answers `Unsupported`. A linked device does so for `proveChatRequest`,
  `prepareDelivery` and `proveDelivery`, because it has no root entropy.
- A call fails with `NotConnected` when no identity is signed in, `InvalidRequest` when the core refuses the request,
  and `Unknown` otherwise.

### Identity

A host is the identity's `Primary` device, the wallet that holds the root entropy, or a `Linked` device paired with it
over SSO.

- `getProfile` (`0`) gives the identity account, the identity's chat public key, the usernames, the role, the statement
  account, this device's encryption public key, and every device of the identity that contacts address.
- The statement account is the identity account on the primary device and the device's own statement account on a linked
  one. The device list holds this device and, on the primary device, the devices paired with it.
- `chatSecrets` (`1`) gives the X25519 secrets of the identity's chat key and of this device's encryption key, because
  the product agrees the per-contact keys and decrypts itself. A linked device's key is the encryption key of its SSO
  session, the key its wallet announced to contacts.
- `proveStatement` (`2`) proves an unsigned statement with the statement account.
- `proveChatRequest` (`3`) signs, as the identity account, a chat request message followed by the SCALE-encoded
  acceptor. The core refuses bytes that are not a chat request message, or whose content names another identity.

### Delivery

- `prepareDelivery` (`10`) derives the delivery account of a request id in the current notification period,
  `//chat-request//<request_id>//<period>` from the root, and claims a notification slot for it.
- It answers `Anonymous` with the period and the account once the statement store takes statements from that account, or
  `NoSlot` when no slot is free or the device is in no ring. On `NoSlot` the product proves the request with
  `proveStatement`.
- A slot that does not take effect in time fails the call, and the product tries again later.
- `proveDelivery` (`11`) proves a chat request statement with the delivery account of a request id and period.

### Pushes

- `pushTokens` (`4`) gives this device's APNs or FCM token and, on iOS, its VoIP token, which the product sends to
  contacts in its `Token` messages.
- `notifyPeer` (`5`) gives the push relay a payload the product encrypted for a contact, addressed to the contact's
  device token under a push id, as a normal or a VoIP push. The host does not read it, and answers whether the push
  service took it.
- `setPushRoutes` (`6`) replaces every route the host's notification receiver reads while no worker runs. A route maps a
  push id to the room the notification opens, the contact's name as its title, and the ChaCha20-Poly1305 key that opens
  the contact's payloads.

### HOP

- `hopUpload` (`7`) uploads a file to a HOP node the host lists, under a ticket the host makes, sealed as the native
  apps seal it. The product's Bulletin allowance account signs the submit and pays when the node moves an unclaimed file
  to Bulletin.
- The product sends the identifier, ticket and node only through its own encrypted channel, because the ticket opens the
  file. It moves the messages too large for the statement store to HOP this way; user media goes through
  [HOP attachments](chat-extension-v2-additions.md#hop-attachments).
- `hopDownload` (`8`) downloads and opens a file from the pool or, once moved, from Bulletin. The core refuses a node
  the host does not list.
- `hopAck` (`9`) tells the node that the entries a download returned were taken, so it drops them.

### Host data

- `publishContacts` (`12`) replaces the contacts the host keeps for the wallet, caller names and the
  [contact picker](contacts-api.md) of other products: the account, the username, the room and whether it is blocked.
- `historyExport` (`13`) gives a part of the native chat history the host staged when it moved its native chat to the
  product. The product reads from offset `0` until `total`, and a `total` of `0` means nothing is staged.
- The core does not read the export. Its format is agreed between the host's native chat and the chat product.
- `historyImported` (`14`) tells the host the history is imported, so it may drop the export and its native chat tables.
- `paymentReceived` (`15`) hands the host the coin keys of a payment a contact sent, as the
  [payments of contact rooms](chat-extension-v2-additions.md#contact-rooms) describe.

## Trade-offs

- The X25519 secrets leave the host, so a faulty chat product can read every conversation of the identity. Keeping them
  in the host would make each host run the key agreement and the decryption of every message.
- The `chat` label is the only gate in the core, and the label already bypasses permission prompts there. The apps
  narrow it: they run the worker they bundle for `chat.<tld>`, checked against a pinned SHA-256, whatever dotNS
  publishes under the name. The Android app also runs a newer dotNS release once its content hash verifies.
- The host keeps the parts of the protocol that need its secrets or credentials: the delivery derivation and the slot
  claim, the HOP ticket and sealing, the push relay and the opening of background pushes. A change to them needs a host
  release.
- The methods serve the chat only. A HOP, push relay or identity signature API for any product would need a permission
  and an abuse model.
