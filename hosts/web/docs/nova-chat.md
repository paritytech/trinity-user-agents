# Nova Chat (prototype)

**Prototype. Not yet tested with a Nova app on a phone.**

Nova Chat (`src/chat/nova-chat/`) is a chat product that speaks the official Nova (Polkadot) apps' person-to-person
chat: their V2 chat requests, their acceptance (`DeviceChatAccepted`, kind 20) and their multi-device route. It runs
from the developer tools' **Chat** tab (dev server, or a build with `WEB_SIGNING_KETTLE_CHAT=1`), for a signed-in wallet
over the network, or for the invitation demo over the simulated Statement Store.

The reference is the apps' source: Android `paritytech/polkadot-android-community` at `fc186861db` (release
`v1.0.0-1080`) and iOS `paritytech/polkadot-ios-community` at `22407b1058`. A newer app build may change the wire.

## Who does what

- **The host keeps every key** of the user's (`src/chat/nova-chat/boundary.ts`): the identity's sr25519 key, its chat
  X25519 key, this device's X25519 key (the `chat-device` ECDH domain), and every secret made from them.
- **The host** (`src/invitations/nova-chat-host.ts`) reads the identity's request topics with its own Statement Store
  client, checks each request (statement proof, inner signature, identity proof against the sender's on-chain chat key)
  and lists it in the Inbox. It sends a request only after **Send** and an acceptance only after **Accept**.
- Each is signed by a notification account the core registers for the day: the apps' `//chat-request//<id>//<day>` and
  `//chat-accept//<their id>//<day>`. A wallet posts only once that account's allowance holds at the People chain's
  finalized head, as Kettle Chat waits for its accounts: a node drops a statement whose allowance is only in a best
  block.
- Claims run one at a time. A request that failed is tried up to five times, 30 s apart, and again at the next start; an
  Accept may be pressed again. A try on the same day reuses the account and claims no new slot.
- New chat asks **Send** before the worker is asked to start, so the prompt does not count against the start's 30 s. The
  worker's `request` spends that consent, for the same person and first message, while New chat waits; otherwise the
  host asks again. A sign-out or a wallet switch closes the prompt as a Cancel.
- The worker's `request` is answered once the host keeps the request, not once it is posted: the finality wait can
  outlast the start's 30 s. **Sent** shows it **Posting**, then **Retrying** with why after a failed try, **Paused**
  once the tries ran out, and **Waiting** once the store took it. A request cancelled under **Sent**, or to a person
  blocked meanwhile, is not submitted.
- Decline in the Inbox forgets the request, and a repost of it is not listed again.
- **The worker** (`src/chat/nova-chat/worker.ts`, `session.ts`) keeps the conversations, history, acknowledgements and
  retries. It follows its conversations' topics and posts through its own TrUAPI Statement Store, one product account
  per conversation, signed with `createProof`. It knows people only by contact handle.
- **`nova-chat/1`** (`src/chat/nova-chat/contract.ts`) is the page-local port between them. Draft, not TrUAPI. The host
  serves it only to `nova-chat.<suffix>`. The worker asks the host to `request`, `seal` a frame for a person, `open` a
  statement it read, give a conversation account its allowance, and to list its people.
- The worker posts one conversation's statements one at a time, with a rising expiry per channel. It acknowledges every
  peer request, also one holding only kinds it does not show, such as the push token both apps send once a chat is set
  up; such kinds are not listed. Its acknowledgement on the device route is what tells the app to stop posting its
  acceptance again.
- While messages wait for an acknowledgement, the worker posts them again once the conversation account's allowance
  ends, with a new one.
- A post that failed is tried again after 30 s, doubling up to 30 min, one timer per chat. A refusal that does not
  change by waiting (a blocked person, a chat that is not set up, another account) stops the tries until the user sends
  or retries, or the chat's state changes. The core's claim errors reach the host as text, so no free slot, no
  personhood and an unreachable chain are all tried again on that schedule.
- A host that stopped (sign-out, another wallet) changes nothing after: no write, no Inbox item, no post.
- One conversation account per chat: the worker derives its index from the contact handle, so a worker that lost its
  storage finds the same account. The host refuses any other index for that chat and never switches it.

Nova Chat does not use the host request service (`host-requests/1`): the two share no wire.

## Checks

- A statement's sender is bound by its topic, never its signer, as both apps do: the apps sign with a per-chat account.
- `open` checks the statement proof, a topic of a device the host verified, the encryption and this device's envelope
  entry. A frame gives the worker only user content: text, rich text without files, reactions, replies, edits, leaving.
  Any other kind is a stub with its id and time.
- `seal` refuses any content but user content. Nothing is sealed or opened for a blocked person, and a request from a
  blocked person never reaches the Inbox.
- This is an API contract, not a sandbox: the worker runs on the host's origin ([Trust model](trust-model.md)). It sees
  the raw statements it reads, so a peer that signs with its identity account (the apps' fallback when no slot is free)
  shows that account to it.

## Not handled

Push, calls, files, payments, history from the apps, a request that crosses one of ours, a new request from an
established contact, device changes, more than one device per identity, Seen and request cancel, and posting a request
or acceptance again on a later day. A request or acceptance takes one notification slot on each day it is posted. An app
that has no push token sends nothing on the device route after it accepts, so it keeps posting its acceptance until the
user writes.

## Tests

- `tests/unit/chat/nova-chat/`: the key boundary's checks, both directions against the independent
  `tests/unit/fixtures/nova-chat/native-peer.py` (run it with `NOVA_PEER=1`), the host side with the worker session over
  an in-memory store, and a New chat through the draft bridge whose finality wait outlasts the start's 30 s.
- [`browser/nova-chat.spec.ts`](../browser/nova-chat.spec.ts): three demo users over the simulated store, both
  directions, with a reload; and, on Playwright's clock, a Send prompt left open past the start's 30 s, cancelled once
  and then sent.

Neither shows that a Nova app reads us. That needs a phone: the app's build and network, and an identity registered on
that network's People chain for the browser, never a phrase the phone also uses.
