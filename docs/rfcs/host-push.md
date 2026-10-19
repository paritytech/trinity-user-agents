---
title: "Product push notifications"
owner: "Leonardo Custodio"
status: draft
---

# RFC: Product push notifications

## Summary

A product gets an opaque push address for one of its rooms and gives it to a contact through its own encrypted
channel. The product of the contact then asks its host to notify that address with a title and a text. The host of the
sender encrypts both for the device of the address and sends them through the push relay of the backend. The device
shows the notification even while the app is closed, and a tap opens the room.

## Motivation

A chat product runs only while its host runs it. When the app of a contact is closed, a new message waits on the
statement store until the contact opens the app again, and nothing tells the contact that it is there. The native chat
of the app solves this with the push relay of the backend (`POST /api/v1/notify`): the sender's app pushes to the
device token of the recipient, with a payload that only the recipient can open. A product cannot call the relay. The
relay needs the device attestation JWT of the app, and a product must never hold it. A wake of the worker of the
product (RFC "Worker wake watches") needs a silent push, which the relay does not send on iOS.

## Approach

A new `Push` trait has two methods.

```rust
pub struct HostPushAddressRequest { pub room_id: String }
pub struct HostPushAddressResponse { pub address: Vec<u8> }

pub enum HostPushAddressError {
    /// The device has no push token now. Ask again later.
    NotAvailable { reason: String },
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

pub struct HostPushNotifyRequest {
    /// A push address that a contact gave.
    pub address: Vec<u8>,
    /// At most 64 Unicode scalar values.
    pub title: String,
    /// At most 256 Unicode scalar values.
    pub body: String,
}

pub struct HostPushNotifyResponse {}

pub enum HostPushNotifyError {
    InvalidAddress,
    InvalidRequest { reason: String },
    /// More than 30 notifications in 60 seconds from this product. Nothing was sent.
    RateLimited,
    /// The push service did not take the notification.
    Rejected { reason: String },
    Unknown { reason: String },
}

#[wire_trait(id = 28)]
pub trait Push {
    #[wire(id = 0)]
    async fn address(&self, cx: &CallContext, request: HostPushAddressRequest)
        -> Result<HostPushAddressResponse, CallError<HostPushAddressError>>;
    #[wire(id = 1)]
    async fn notify(&self, cx: &CallContext, request: HostPushNotifyRequest)
        -> Result<HostPushNotifyResponse, CallError<HostPushNotifyError>>;
}
```

**The address.** The host keeps a route for each room of each product: a random push id (16 bytes, as 32 lowercase hex
characters) and a random 32-byte key. The notification service extension of the app can read the routes. The address is
the version byte `1` and the SCALE of `{ service, token, push_id, key }`, where `service` is `Apns` or `Fcm` and `token`
is the push token of the device. The address of a room stays the same until the device gets a new token. The product
treats it as opaque bytes and sends it only to the contact of the room.

**The notification.** The core of the sender checks the address and the limits, then encrypts the JSON
`{"v":1,"title":…,"body":…}` with XChaCha20-Poly1305 under the key of the address, with the associated data
`truapi/push/v1`. The relay gets the push token, the push id and the hex of the 24-byte nonce followed by the
ciphertext. It never sees the room, the title or the text.

**The device of the contact.** The notification service extension finds the route by the push id, opens the message
with its key and shows the title and the text. The tap opens `polkadot://<product>/-/chat/<room>`. A push id that the
device does not know, or a message that does not open, shows nothing of the product.

**The relay call.** The host calls the relay with its own JWT, as the native chat does. The product sees neither the
JWT nor the response of the relay, only whether the push service took the notification.

| Answer           | What the product does next                                  |
| ---------------- | ----------------------------------------------------------- |
| address `NotAvailable` | Asks again later, for example after a few minutes.    |
| `InvalidAddress` | Forgets the address. The contact sends a new one.           |
| `RateLimited`    | Sends no more notifications for a while.                    |
| `Rejected`       | Waits for a new address from the contact.                   |
| `Unsupported`    | Sends no notifications. This host has no push relay.        |

**No permission.** A notification goes only to a device whose owner gave the address to the product, and its text is
what the sending product writes. The rate limit bounds a product that misbehaves. The user of the receiving device
controls the notifications of the app in the settings of the system.

**Hosts.** The iOS app keeps the routes in the shared keychain, and its notification service extension opens product
pushes. Android, the web hosts and the CLI host roles do not install a push platform yet and answer `Unsupported`.

## Trade-offs

- The sending product writes the text, so a product can put the text of a message in the notification. A product that
  wants less in the notification writes less.
- The key travels inside the address, in the encrypted channel of the product. Anyone who reads the address can push
  to the room. The address goes only to the contact, through the product's own end-to-end channel.
- The relay sends alert pushes only. A notification does not wake the worker of the product; the product reads the
  message when the app runs again.
