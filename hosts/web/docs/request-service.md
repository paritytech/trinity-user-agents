# Request service and invitation demo (prototype)

The request service (`host-requests/1`, `src/invitations/contract.ts`) lets two hosts set up a chat. It follows the v3
"Establish Connection" design: the product gives an opaque payload, and the hosts own the request, the return route and
the keys, and authenticate both directions. Nothing in it is an agreed API.

## API

The product hands the host a `MessagePort`. It names a contact by the `ContactHandle` from `contacts.pick`. Each call
has an `id` and one `result`.

| Side                                  | Calls                                                      | Host pushes                               |
| ------------------------------------- | ---------------------------------------------------------- | ----------------------------------------- |
| Sender                                | `request` (handle, purpose, payload), `complete`, `cancel` | `outgoing` (status), `response` (payload) |
| Recipient, after the user's Accept    | `reply`, `refuse`, `open`                                  | `acceptIntent` (request, sender, payload) |
| Either, after Send + reply, or Accept | `chatAccount` (request, 32-byte index)                     |                                           |

- `complete` is allowed only after an authenticated reply. It stops reposts and sends nothing.
- Statuses: `waiting`, `paused`, `completed`, `cancelled`. A request has no deadline. There is no delivered, declined or
  expired status.
- Products never see a request id, route, reply key, evidence or identity. The recipient's product does not get the
  purpose.

## Ownership

- The sender's host makes the request id and a reply key pair per request. It stores the secret before the first post,
  and drops it on complete or cancel. After that no reply opens. Dropping is not a secure erase.
- Tabs merge each other's changes before they write, but writes are not atomic across tabs.
- The delivery (`HostDelivery`) chooses both routes. The sender's host binds the return route into the authenticated
  request. The recipient's host replies only on that route, never on one from the payload.
- The sender's host matches a reply to the request whose reply key opens it. A post that opens for none is dropped
  silently.

## Sealing

- HKDF-SHA256 and ChaCha20-Poly1305 (`src/invitations/sealing.ts`).
- A request is sealed with X25519 (classical) to the recipient's published chat key. The host proof is also classical.
  So the recipient needs no published post-quantum key.
- A reply is sealed to the reply key in the request. The key length selects the suite (`src/invitations/binding.ts`):

| Reply key   | Suite                                                                     | Use                      |
| ----------- | ------------------------------------------------------------------------- | ------------------------ |
| 1,216 bytes | X-Wing KEM (X25519 + ML-KEM-768, draft-11, `@noble/post-quantum`), hybrid | Default for new requests |
| 32 bytes    | X25519, classical                                                         | More → Reply key (dev)   |
| Other       | Refused                                                                   |                          |

- The suite is part of the authenticated request. A later change of the setting does not affect sent requests. The KEM
  is checked against the draft's test vectors.
- **Sizes** (payloads are hex, two bytes per byte; test fixture context):

| Item                                               | X-Wing              | X25519            |
| -------------------------------------------------- | ------------------- | ----------------- |
| Sealed request                                     | 3,153 + 2 × payload | 785 + 2 × payload |
| Sealed reply                                       | 1,772 + 2 × payload | 683 + 2 × payload |
| Max request payload in 10,240 bytes, short purpose | 3,543               | 4,727             |
| Max reply payload in 10,240 bytes                  | 4,234               | 4,778             |

## Host proof

- v3 `AuthEvidence`: `kind`, a 32-byte `identityMac` (BLAKE2b-256 keyed with the X25519 secret of the two chat keys) and
  a 64-byte sr25519 `messageSignature` by the identity account (`src/invitations/wallet-auth.ts`).
- Before it proves anything, the host checks that its derived keys match the core's session, and that the directory
  publishes that chat key for that account.
- A request's evidence covers the version, both identities, product, purpose, request id, times, return route, public
  reply key and payload. A reply's covers the version, both identities, product, request id, request digest and payload.
- The kind and identity order stop one direction's evidence from passing as the other's.
- The recipient's host checks a request's sender against the chat key that the directory publishes for the sender's
  identity, never against a key that the request supplies.
- The sender's host checks a reply against the contact that it chose, never a key the reply names. A refused reply does
  not use up the request.
- **It is a pilot.** The covered bytes are this repository's profile. The design leaves the preimage, encoding and
  labels open. It is classical, shows one identity to every contact, and has had no security review.
- The identity signature is inside the sealed envelope. It never signs a public post.
- Contact Chat adds its own check in `simple-text/1`: the response repeats the invitation's random challenge.

## Delivery

- **Network delivery** (`src/invitations/statement-delivery.ts`, DRAFT): the default for a wallet. Notification accounts
  sign the posts, and the chain checks the allowance.
- **Local delivery** (`fixtures/invitations/local-delivery.ts`): demo users and "this browser" modes. Posts stay in
  `localStorage`. Nothing signs them, and nothing checks an allowance.

### Network delivery

- Every request and reply on a network goes to one Statement Store topic per UTC day, with nothing named in the clear.
  Each host reads today's and yesterday's topic and tries to open everything.
- Each post is one statement signed by a fresh sr25519 notification account per post and day, hard-derived from the
  wallet's entropy. The core registers it with the user's personhood proof
  (`Resources.set_notification_statement_account_for_sequence`, through `claimNotificationAccount`).
- Such an account may post one statement of at most 10,240 bytes (`NotificationAllowance`, read 2026-10-30). A sealed
  Contact Chat request is about 1.2 to 1.5 KB, a reply about 1.4 KB. A payload above about 4.5 KB does not fit. The host
  refuses an oversized one (`Invalid`, with size and limit) before it asks the user.
- A pending request is posted again each UTC day with a new account. A failed claim or submit pauses it and retries
  every 15 minutes. The same post on the same day derives the same account, so a restart claims nothing new.
- "Network: claim and post" claims and posts for real. Each post takes one daily notification slot. With none left, the
  request pauses until a later try.
- "Validate claims only" builds every claim, asks the runtime, and submits nothing. The tab's network also refuses every
  submit and broadcast before it leaves (`tools/demos/no-submit.ts`).
- While it is selected, the picker does not offer saved contacts. A person found by username gets a minted handle for
  the request. A name does not make that person payable.
- Not interop-tested with the apps. Not the design's Backhaul.

### Local delivery

- Each host has an inbox in `localStorage`, labelled from its agreement key. A return route is the sender's inbox and a
  random tag. The recipient replies only to the sender's inbox. The tests also run a random topic per request.
- Posts repeat every minute and lapse after five minutes. These are dev choices, not answer deadlines. A refused post
  pauses the request, which retries.
- Inbox labels are linkable, tags are in the clear, and any script of the origin can read or post.

### Retries

- A reply has no deadline. The recipient posts the same sealed reply whenever the request arrives again. The delivery
  retries each reply post for an hour (`REPLY_POSTING_MS`).
- A request's time is for display. No time refuses it.
- When a contact's chat key cannot be read, the request pauses and retries every minute. A definite answer (no key,
  another key type, a conflict) is kept until the next read. When the chain does not answer the check of the wallet's
  own key, the check retries every 30 s.

## Invitation demo

**FIXTURE IDENTITIES and LOCAL SIMULATED DELIVERY.** A tab can act as Alice, Bob or Carol, to try features that need
several people.

1. Tab 1: Developer tools → **More** → **Invitation demo** → **Alice**. The tab reloads as Alice and opens nothing.
2. Tab 2: choose **Bob**.
3. Alice: Chat **+** → Bob → **Send request to Bob**. Bob's Inbox shows 1. Bob chooses **Accept**. Inspector → Activity
   shows each step.

- The core signs a demo user in with the public test vector of that name, under `test-user:<id>`.
- **Off**, **Sign out**, or a picker sign-in leaves the demo. Saved wallets stay untouched.
- **Clear Alice's invitations** removes that user's requests and Inbox items on this network, after a confirmation.
- No wallet service starts: no saved contacts, wallet worker or Pocket. So fixture contacts never reach a wallet or a
  transaction.
- Fixture contacts (`fixtures/invitations/test-users.ts`): Alice has Bob and Carol. Bob and Carol have Alice. Handles
  match `contacts.pick`, because the contacts use each user's derived identity account.
- Storage per network and demo user: `truapi-web-signing-host:network:<id>:test-user:<user>:notifications:v2`,
  `...:contact-chat-outbox:v2` and `...:contact-chat-received:v2`. Older `...:outbox` and `...:received` lists stay
  unread, and Clear removes them.
- Conversations from before the handle change show their person as **Contact**.

### What is simulated

- **Handles.** The host mints handles for every other demo user and names them from the fixture directory. A name is not
  permission: Carol, known to Bob only through the directory, is drawn but never paid.
- **Chat keys.** `testUserDirectory` answers as the People chain would. A real host reads `chainDirectory`, with the
  same lookups and refusals. The directory holds every test user, so a host accepts requests from any of them.
- **Proof.** The phrases are public. A proof shows that the checks work, not who a person is.

## Code

- `src/invitations/`: `contract.ts`, `ports.ts`, `binding.ts`, `sealing.ts`, `wallet-auth.ts`, `published-keys.ts`,
  `directory-contacts.ts`, `statement-delivery.ts`, `invitation-service.ts`, `outbox.ts`, `received.ts`,
  `contact-chat-host.ts`, `wallet-chat.ts`, `chat-keys.ts`, `gate.ts`, `observer.ts`.
- Dev and test only: `fixtures/invitations/`, `fixtures/notifications/`, `tools/demos/dev-tools.ts`.
- End-to-end unit tests: `tests/unit/invitations/invitation-flow.test.ts` (two and three users),
  `statement-delivery.test.ts` (network delivery, in memory), `tests/unit/chat/contact-chat/contact-chat.test.ts`.
