---
title: "Sharing between users"
owner: "@pgherveou"
status: draft
---

# RFC — Sharing between users

Interactive flows: [Product Messages in Chat](https://claude.ai/artifact/LGne7mj44Y9hcqJpohR6gU).

## Summary

- A product can share profile data with a contact, or with a public DotNS name through its sharing inbox.
- Users can share profile data, and request access from another user.
- Products never receive contacts' DotNS names or profile contents. The host keeps them and shows them in UI it
  controls.

## Motivation

For a user to interact with other users from within a product, the product needs to request access to their product's
accounts and to their profile information.

## Flow

### Sharing with a contact

Alice wants Bob, one of her contacts, to join her in a product. Her host sends the sharing request into their chat. Bob
sees it there, chooses what to share back, and opens the product, installing it first if he doesn't have it yet.

```text
 Product (Alice)                Alice's host                 Statement store                 Bob's host                 Chat
        |                             |                             |                             |                       |
        |------- contacts.pick ------>|                             |                             |                       |
        |<.... Picked { handle } .....|                             |                             |                       |
        |------- sharing.send ------->|                             |                             |                       |
        |                             | Alice confirms              |                             |                       |
        |<...... { shareId } .........|                             |                             |                       |
        |                             |------- ShareRequest ------->|                             |                       |
        |                             |                             |------- ShareRequest ------->|                       |
        |                             |                             |                             |--------- card ------->|
        |                             |                             |                             |<-- Review & share ----|
        |                             |                             |<------ ShareResponse -------|                       |
        |                             |<------ ShareResponse -------|                             |                       |
        |<------ ShareResponse -------|                             |                             |                       |
        |                             |                             |                             |                       |
```

1. Alice picks Bob in the host's contact selector. A product gets a handle.
2. The product calls `sharing.send` with that handle. Alice confirms the request in a confirmation modal displayed by
   the host.
3. Alice's host posts the sharing request into her chat with Bob.
4. Bob reviews the profile information he agrees to share, and accepts.
5. Bob's host sends `ShareResponse` over the chat.
6. The product receives `ShareEvent::Answered` through `sharing.subscribe`

### Sharing with a public DotNS name

Alice shares with someone who isn't a contact, such as a shop she found in Market (`market.dot`). Her host sends the
sharing request to the shop's sharing inbox; it never appears in chat.

See [Sharing inbox](#sharing-inbox) for how requests reach a DotNS name.

```text
 Market (Alice)                 Alice's host                 Statement store                Acme's host           Market (Acme)
        |                             |                             |                             |                       |
        |------- sharing.send ------->|                             |                             |                       |
        |                             | Alice confirms              |                             |                       |
        |<...... { shareId } .........|                             |                             |                       |
        |                             | looks up acme.dot           |                             |                       |
        |                             |------- ShareRequest ------->|                             |                       |
        |                             |                             |------- ShareRequest ------->|                       |
        |                             |                             |                             | keeps grants          |
        |                             |                             |                             | notifies Acme         |
        |                             |                             |                             |------ Received ------>|
        |                             |                             |                             |<--- opens route ------|
        |                             |                             |                             |----- draws data ----->|
        |                             |                             |                             |                       |
```

1. Market calls `sharing.send` with `to: DotNs("acme.dot")` and the scopes Alice gives in `give`. Alice confirms in a
   host modal.
2. Alice's host looks up `acme.dot` and posts the sharing request to its sharing inbox.
3. Acme's host picks the request up from its sharing inbox, keeps the grants, and shows Acme a notification with the
   request's title.
4. Market on Acme's device receives `ShareEvent::Received`, with the request's `deeplink_route`.
5. When Acme opens that route in Market, its host fetches and decrypts the shared data and draws it. Market never sees
   it.

## Detailed Design

Builds on
[RFC 0002, Chat Extension v2](https://github.com/paritytech/trinity-user-agents/blob/517e9e23e193f2341dfaadac2f0626f7eb1d1436/docs/rfcs/0002-chat-extension-v2.md).

### 1. The product sends the sharing request: `sharing.send` (new)

```rust
/// Exchanges between users, carried from host to host. Callable from SPAs and Workers.
pub trait Sharing {
    /// Sends a sharing request once the user confirms it in a host modal.
    async fn send(
        &self,
        cx: &CallContext,
        request: HostSharingSendRequest,
    ) -> Result<HostSharingSendResponse, CallError<HostSharingSendError>>;
}

pub struct HostSharingSendRequest {
    pub to: Recipient,
    pub title: String,
    pub deeplink_route: String,
    pub scopes: ShareScopes,
}

pub enum Recipient {
    Contact(ContactHandle),
    DotNs(String),
}

pub struct ShareScopes {
    /// The sender's own data, shared once the sender confirms.
    pub give: Vec<ProfileScope>,
    /// The recipient's data the sender asks for.
    pub ask: Vec<ProfileScope>,
}

/// A part of the user's Seity profile.
pub enum ProfileScope {
    Public,
    Shipping,
    // ...
}

pub struct HostSharingSendResponse {
    pub share_id: String,
}

pub enum HostSharingSendError {
    /// Alice cancelled the confirmation modal.
    Rejected,
    // ...
}
```

### 2. What the host sends to the statement store

Alice's host finds her chat with Bob from the handle and posts the sharing request there, stamped with the calling
product's id.

```rust
pub enum MessageContent {
    ShareRequest(ShareRequest), // new
    // ...
}

/// The sharing request as it travels between hosts.
pub struct ShareRequest {
    /// The sender's product's account.
    pub from: AccountId,
    /// The calling product, from the call context.
    pub product_id: ProductId,
    pub title: String,
    pub deeplink_route: String,
    /// The recipient's data the sender asks for.
    pub ask: Vec<ProfileScope>,
    /// Decrypting material for each scope the sender gives.
    pub grants: Vec<ProfileGrant>,
}

/// Lets the receiving host read one scope of the sender's profile.
pub struct ProfileGrant {
    pub scope: ProfileScope,
    /// Seity record on Bulletin.
    pub record: String,
    /// Opens that record, for this grant only.
    pub key: [u8; 32],
}
```

### 3. How the recipient's host receives it and replies

The recipient's host keeps the grants. When `ask` is not empty, it draws the card and posts the answer back in the same
chat, like a reaction. Answers travel in chat, so `ask` needs a `Contact`.

```rust
pub enum MessageContent {
    ShareResponse(ShareResponse), // new
    // ...
}

/// The recipient's answer, sent only when the sharing request has an `ask`.
pub struct ShareResponse {
    /// Id of the sharing request message.
    pub share_id: MessageId,
    pub outcome: ShareOutcome,
}

pub enum ShareOutcome {
    Accepted {
        product_account_id: AccountId,
        /// Decrypting material for each scope Bob agreed to share.
        grants: Vec<ProfileGrant>,
    },
    Declined,
}
```

Alice's host keeps the grants against Bob; any product that can identify Bob can have the host show his data.

#### Sharing inbox

Where a host receives sharing requests from people who aren't contacts. It works like the chat-request inbox, except:

- **Topics:** `"sharing-inbox"` in place of `"chat-request"`, with the account from the username's DotNS record.
- **Sender:** signed by the sender's product's account, not their identity.
- **Product:** the product id travels inside the encrypted request, so observers can't tell which product it is for.
- **Delivery:** the host passes each request to the product if its user uses it, and drops it otherwise, without telling
  the sender. Nothing reaches the chat list.

### 4. How the products hear about it: `sharing.subscribe` (new)

Hosts pass sharing requests and answers to products without the grants.

```rust
pub trait Sharing {
    /// Share events for the calling product, live or replayed on its next start.
    async fn subscribe(
        &self,
        cx: &CallContext,
        request: HostSharingSubscribeRequest,
    ) -> Subscription<ShareEvent, CallError<HostSharingSubscribeError>>;
    // ...
}

pub enum ShareEvent {
    /// Sender side: the recipient answered.
    Answered { share_id: String, answer: ShareAnswer },
    /// Recipient side: a sharing request for this product arrived.
    Received { share_id: String, from: AccountId, deeplink_route: String, shared: Vec<ProfileScope> },
}

/// The recipient's answer as products see it: scopes, never grants.
pub enum ShareAnswer {
    Accepted { account_id: AccountId, shared: Vec<ProfileScope> },
    Declined,
}
```

### Contact components — placeholder

To be written. Direction:

- Products show people's shared data through components the host controls and renders inside the product view.
- How they are rendered is left to a later RFC.
- The product places them but never receives what they display.
