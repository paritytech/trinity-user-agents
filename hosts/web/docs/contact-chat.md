# Contact Chat (prototype)

**Prototype. It uses the network. Messages are NOT ENCRYPTED.**

Contact Chat (`src/chat/contact-chat/`) is the host's own chat product. It is a worker that knows people only by the
contact handles that its host gives it. It runs for every wallet signed in from the Account picker, in every build.

Two registered wallets can find each other in Chat's **+** by full username, send a request, accept it in the Inbox and
write text. Requests travel through the [request service](request-service.md).

## Requirements

- The wallet's identity account has a chat key on chain, and personhood for the daily slots. Register a username in the
  Polkadot app to publish the chat key, then sign in again.
- The derived keys must match the identity account and chat key that the core reports and the chain publishes. Otherwise
  requests stay off, and New chat and the log say why.
- The Substrate development phrase is refused. The test accounts work, but their phrases are public.
- A person is any identity account with a published chat key, except a blocked one.

## At sign-in

- The host starts the worker and the request service (`src/invitations/wallet-chat.ts`,
  `src/invitations/contact-chat-host.ts`).
- The service reads the network's day inbox and posts again this browser's pending requests. Another browser does not
  see them.
- Nothing leaves without the user: a request goes only after **Send invitation**, a reply only after **Accept chat
  request**.
- The host builds the worker archive itself (`contact-chat-plugin.ts`): `contact-chat/worker.json` under the base, and
  blocks at `/ipfs/<cid>` on the origin. The loader checks them. Nothing is published to DotNS or Bulletin.

## Cost

Both prompts show one line about daily slots and public messages. It opens the details.

- Sending or replying claims one of the person's daily notification slots and posts one sealed statement.
- A pending request is posted again on each later UTC day, with a new slot, until it completes or is cancelled under
  **Sent**. A reply is posted again when its request arrives again.
- Each conversation claims one daily Statement Store slot for its own account on each day that it posts.
- With no personhood or no slot left, the request pauses and says why.

## How a chat runs

1. Alice chooses **+** → Bob → **Send request to Bob**. The host gives the worker Bob's handle
   (`startConversation { handle }`).
2. The worker makes the invitation payload. The host's **Send invitation** prompt asks first.
3. Bob's **Accept** goes to his worker. It answers, makes both topics, reads `T_AB` and replies.
4. Alice's host authenticates the reply. Her worker checks it, reads `T_BA`, and calls `complete`: **Connected**.

- Each worker saves its session, and waits for the storage to confirm, before its host sends, replies or completes.
- Neither worker gets a name, username, account, identity key, request id or route. Each host names the other person
  from the handle.
- The worker can also call `contacts.pick` and send to the handle it gets, and cancel its own request. Only the browser
  test drives these.

## Start over

**Chat options** (**⋯**) → **Start over** is for a connection whose messages no longer get through.

- It sends a new request on a new conversation account, through the usual prompt and costs. The other side must accept
  exactly this request. It replaces their connection too.
- Nothing changes until the host holds the request. Then the room keeps its history, marks the end of the old
  connection, and stops reading it.
- Messages still sending in the old connection show failed. Old messages are not posted again. Nothing tells the old
  connection.

## Text (`simple-text/1`)

- Each side keeps one statement up, on its own topic (Alice `T_AB`, Bob `T_BA`), signed by its conversation account.
- A post holds the side's last 20 messages, fewer past 16 KiB, and replaces the last post.
- The store replaces a statement on the same channel and account only with a strictly higher expiry. So each expiry is
  two days ahead, in its high 32 bits. The node refuses only an expired statement (polkadot-sdk `06ccd162f`, read
  2026-10-31).
- A general allowance keeps two statements and 512,000 bytes per account (read 2026-10-30). The network keeps a short
  window. Each worker keeps the full history in its own storage.
- A peer away for more than 20 messages, or past the expiry, loses the older ones.
- A message that was sending when the worker stopped shows failed after a reload, with Retry.
- **Not encrypted.** Anyone who reads the Statement Store sees the text, topics, channels, both accounts, sizes and
  times, and can link one account's posts. Only the setup is sealed. The room and the prompts say so.
- **Reading.** By default People runs on its RPC node, which replays what it holds. On the light client backend, a
  subscription's first page is always empty, and an earlier post arrives only if a peer sends it then. There, `new`
  means that a peer took the broadcast, not that it stored it. The unit tests' in-memory store replays like a node.
- **Protocol boundary.** `protocol.ts` owns the setup payloads, topics, channels and post bytes. The engine owns the
  conversation, account index, history and storage. The hosts carry payloads unread. Another protocol (MLS, for example)
  would replace `simple-text.ts` with no host change.

## Conversation accounts (draft)

- A conversation account is Contact Chat's product account at a random 32-byte `Raw` derivation index, kept in its
  storage. The worker gets the public key with `account.getAccount` and signs with `statementStore.createProof`.
- Before it posts, the worker asks its host to allow the index (`chatAccount`). Consent is the user's Send (after an
  authenticated reply) or Accept. The first index binds to the request, so one consent funds one account. A product
  funds only its own accounts.
- On the network delivery, the core gives the account a general Statement Store slot of the wallet's personhood for the
  UTC day (`claimProductStatementAccount`). It reuses a slot that the account already holds. It never replaces a live
  slot, and submits at most once. After a failed submit, an absent account is reported uncertain.
- One slot per account per UTC day: 50 per day for a full person, 10 for a lite person (read 2026-10-30).
- The local delivery and demo users give no allowance. "Validate claims only" modes submit nothing and say which slot
  they would take.
- A failed post fails its messages. Contact Chat adds the core's reason. The message's own "Not sent" line has none,
  because the draft contract carries none.

## Diagnostics (draft)

- The worker reports each conversation to its host, for the [Chat view](inspector.md#chat). A report holds the protocol,
  state, request handle, account, peers, topics and counts.
- Protocol details for `simple-text/1`: the last post's batch against the limit (20 messages or 16 KiB), and the 48 h
  expiry. Peers may evict sooner.
- No report carries message text.
- A report is a `product-diagnostics/draft-1` window message (`src/products/diagnostics.ts`). The host reads it within
  bounds, assigns it to the sending frame, and acts on none of it. A stopped worker's report is stale.

## Dev server controls

The developer tools' **Chat** tab (`tools/chat/products.ts`) holds them, for this tab, in its sessionStorage:

- **Chat products:** an On switch for Contact Chat (on by default) and for
  [Kettle Chat](#kettle-chat-dev-server-prototype) (off by default). Only the products that are on start, and Chat shows
  only their chats. Each row shows the product id, whether its messages are encrypted, and its worker's state and
  waiting sent requests, as the page holds them; **Inspect** opens its worker in the inspector.
- **New chats start in:** the product New chat sends a request through while both are on (Contact Chat by default). Each
  other product keeps its chats but starts none.
- **Off is not deleted.** A product that is off does not start: no worker, no request service, no delivery and no
  reposts. Its chats, history, request lists, storage and accounts stay, and it starts from them when it is on again.
  Its pending sent requests stay pending but are not posted again, so turning it off asks first when some are waiting. A
  request sent to it meanwhile is read only if it is still posted when it is on again, and an invitation already in the
  Inbox shows without Accept.
- Every change reloads the tab, so a product never stops halfway. A tab whose delivery was Off before these switches
  runs no chat product.
- How host requests travel, and the reply key of new ones, are the request service's, which every product that sets up
  through it uses: More → **Request delivery** (the network by default, the network with claims only validated, this
  browser, or this browser with chat account claims validated) and **Reply key for new requests** (X-Wing by default, or
  X25519). See [Request service](request-service.md).

## Kettle Chat (dev server prototype)

The Chat tab's **Kettle Chat** switch runs Kettle Chat (`src/chat/kettle-chat/`), a chat product of its own, alone or
beside Contact Chat: product id `kettle-chat.<network suffix>` (`kettle-chat.paseo` on Paseo Next v2), its own worker,
request service and request lists (`…:kettle-chat:sent|received:v1`), and its own storage and conversation accounts,
which the core keys by the product id. With both on, New chat starts in the product the Chat tab names; each product's
rooms are listed under its own provider. A request carries its product id, so each host hands it to that product only.
It runs Kettle (`paritytech/kettle-core`, pinned in `kettle-chat-wasm/Cargo.toml`): MLS groups over Statement Store
outboxes, end-to-end encrypted.

- Build its WebAssembly once: `npm run build:kettle` (Rust, `wasm32-unknown-unknown`, `wasm-pack`, SSH access to the
  private repository). Without it, the worker's archive is not served and says why. iroh's TLS needs `ring`, which needs
  a C compiler with the wasm32 target. Apple's clang has none, so on macOS use Homebrew's LLVM:
  `CC_wasm32_unknown_unknown=$(brew --prefix llvm)/bin/clang AR_wasm32_unknown_unknown=$(brew --prefix llvm)/bin/llvm-ar npm run build:kettle`.
- **A signed-in wallet** runs it on the wallet's delivery, the network by default: real requests and replies on the day
  inbox, real claims, and Kettle's statements on People. Both sides must choose Kettle Chat. It costs what Contact Chat
  costs: one notification slot for the request and one for the reply, and one general slot per conversation account per
  UTC day. The prompts say the messages are end-to-end encrypted.
- **The invitation demo** runs it over a Statement Store and allowance simulated in this browser
  (`fixtures/statements/local-statement-node.ts`, two statements per account, proofs checked). Nothing reaches the
  network there.
- One Kettle node runs per conversation account, as for `simple-text/1`: `Raw` index, `createProof`, `chatAccount`.
  Entropy and Kettle's sealed state are bound to the index, so no two conversations share an MLS identity or state.
  Kettle checks every statement's proof itself, and the engine drops a statement with more than four topics before
  Kettle reads it.
- The setup payloads carry only the conversation account: `0x4b06` and 32 bytes. Bob replies on Accept, then starts his
  node, which posts his key package. Alice, after the authenticated reply, completes the request at once, shows the
  request's message as sending and the chat as "Setting up the encrypted chat", then makes the group with that first
  message (or the first one she writes) once her node starts. If her allowance does not finalize in time, the message
  fails with the reason and Retry tries again; the request stays completed. A request sent with no message leaves her
  chat setting up once her node runs, saying "Write a message to start the encrypted chat": Kettle makes the group with
  a first message, never with the node. Either side's chat is set up, and says so once, only when its direct group is
  made, joined or restored. Each side keeps only the direct group of the two conversation accounts, made by the peer (or
  by itself, on reload), and leaves any other.
- **Writing before the group.** Bob may write, and Retry a failed message, while he waits for Alice's Welcome. Kettle
  Chat holds those messages, shown as sending and kept across a reload, and hands none to Kettle until Bob joined that
  group: a send before then would make a second group of his own. Once he joined, it sends them in the order written,
  each once. Before Bob's Accept went through, Retry is refused.
- **A second request between the same two people.** An Accept from someone this side already chats with, or sets up a
  chat with, is refused, and the chat keeps running. When both sent a request, both keep the request of the lower
  conversation account: the side that sent it refuses an Accept of the other's, and the other side accepts it and
  withdraws its own at the host, so it is not posted again. A request the host answers after a newer one replaced it is
  withdrawn too. A conversation that starts again removes its old account's sealed Kettle state and keeps its history.
- **Finality guard.** For a signed-in wallet, each conversation account posts only once its allowance holds at the
  People chain's finalized head (`tools/chat/kettle.ts`, read every 3 s, at most 180 s). A node accepts a statement
  against the allowance at its best block but checks it at the finalized block later, so a statement posted in between
  may be accepted and then evicted. This guard is Kettle Chat's only; Contact Chat's first post after `chatAccount` has
  the same exposure.
- **Storage.** Both live in the product's TrUAPI storage, which this host keeps in the browser's `localStorage`, under
  the wallet, the network and the product. The history and the conversations (`kettle-chat:record:v1`) are plain JSON;
  Kettle's MLS state (`kettle-chat:node:<index>:…`) is sealed with ChaCha20-Poly1305 under a key from the product's
  entropy. A reload or a new tab of the same browser and wallet keeps both. Another browser or device has neither: the
  network keeps only Kettle's latest outbox per member, for up to 7 days, and it holds no history.
- The core asks once, before the first post, to let the product post (`StatementSubmit`).
- Earlier builds ran Kettle Chat under Contact Chat's id. What they kept stays where it was: its record
  (`kettle-chat:record:v1`) in Contact Chat's product storage, which neither product reads now, and its requests in
  Contact Chat's lists. Its conversation accounts belong to Contact Chat's id, so it is not moved to Kettle Chat.
- Kettle is licensed AGPL-3.0-only. Its WebAssembly is built locally. A normal build leaves it out;
  `WEB_SIGNING_KETTLE_CHAT=1 npm run build` packs it beside Kettle Chat's worker, with the Chat tab, for a private test
  deployment. That build holds no invitation demo, fixture, simulated store or dev request control.
- **Log recovery over iroh (off by default).** Kettle's `iroh` feature is compiled in (iroh 1.4.1, iroh-blobs `cd2780e`,
  as kettle-core's lock pins them). The Chat tab's **Kettle Chat: recover missed messages** section turns it on for this
  tab (sessionStorage, Apply reloads the tab), on **Use public test relays** by default, or on one **Custom relay
  (advanced)**. The host only passes the choice to the worker in its page's address (`?iroh=public` or
  `?iroh-relay=<url>`); each conversation's node binds its own iroh endpoint on those relays, waits up to 10 s until its
  address names one of them as its home relay, and runs Kettle's log transfer. A peer whose outbox copy no longer holds
  the envelopes it missed then fetches that sender's log, verified against the signed log hash, end-to-end encrypted,
  through a relay. In a browser every iroh connection goes through a relay: no direct UDP. It does not replace the
  Statement Store: keys, Welcomes, recent envelopes and acknowledgements stay there, and with the sender still posting
  the Statement Store alone recovers the same messages. iroh adds recovery when the sender's newer outboxes do not reach
  the store, as long as the sender's tab runs.
  - **Public test relays:** n0's four relays in iroh 1.4.1's default map (`iroh::defaults::prod`):
    `https://{use1-1,usw1-1,euc1-1,aps1-1}.relay.n0.iroh.link.`. They are shared, rate-limited, without guarantee, and
    meant for development and testing (iroh's relay docs, read 2026-11-05). The endpoint uses only that relay map with
    iroh's `Minimal` preset: not the `N0` preset, which also publishes and resolves addresses through n0's DNS (pkarr).
    Before it picks a home relay, the endpoint probes all four over HTTPS.
  - **Custom relay:** a URL of a scheme, a host and a port only. The Chat tab, the worker and the WebAssembly each
    refuse a user, password, path, query or fragment, plain or percent-encoded, with a fixed message that does not
    repeat the value. The URL is not a credential: every outbox publishes it.
  - **Allowed relays only.** iroh would dial whatever relay a peer's address names. Each endpoint installs a
    `before_connect` hook that accepts a peer only when every relay its address names is one of the endpoint's own (any
    of the four public relays, or the one custom relay; a final dot in the host is ignored) and it names no custom
    transport. It refuses before any packet leaves, so a peer cannot make this browser reach a host of its choosing.
    iroh 1.4.1 learns a remote's relay only from the address passed to `connect` and from address lookups, of which the
    endpoint has none (read from iroh's source). A side on the public relays and one on a custom relay never connect:
    their chat runs on the Statement Store alone.
  - **Off:** no endpoint, no relay connection, no address in the outboxes, no socket. The compiled-in feature still
    keeps each sent envelope in an in-memory iroh-blobs store for the node's lifetime, and a second store runs a 60 s
    garbage-collection timer; memory grows with the messages a node sends until it stops.
  - **No relay reached:** that node starts only after the 10 s wait, then runs on the Statement Store alone; until then
    the conversation cannot send. Inspector → Chat shows "unavailable" with the reason, and the worker reports it. A
    reload tries again. Conversations start in parallel, so the wait is not per conversation.
  - **Visible:** each outbox on the Statement Store carries the home relay's URL and an endpoint id, random per node
    start; the relay sees the browser's IP address, endpoint ids, timing and sizes, and on the public relays that is n0.
    A node serves its log to anyone who knows its address and the log hash. A custom relay that few use links a tab's
    conversation accounts for anyone who reads the Statement Store, and its operator can link them by IP address.
  - **Cost:** Kettle Chat's WebAssembly grows from 1.96 MB (0.62 MB brotli) to 4.60 MB (1.43 MB brotli), on or off.
  - A local relay for tests: `cargo install iroh-relay@1.4.1 --features server --locked`, then `iroh-relay --dev`
    (`http://127.0.0.1:3340`).
  - It stores nothing of its own: turning it on or off changes whether a node binds an endpoint, not the chats, history
    or Kettle's saved state. `kettle-iroh.spec.ts` turns it on and off mid-chat.
- Gaps: no Start over, so a peer that lost its state cannot start a new chat with someone who still has the old one;
  with iroh off, a peer away past Kettle's outbox window gets the missed messages only as the sender posts them again,
  and with iroh on only while the sender's tab runs; each running conversation republishes its keys, so it claims its
  slot every day it runs.

## Gaps

- A chat from the host's **+** asks twice: in the request form and in **Send invitation**.
- A declined request waits until the sender cancels it.
- No receipts. A sent message never shows delivered or read.
- Text over the network has not been tried between two hosts yet.
