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

More → **Contact Chat delivery** selects: the network (default), the network with claims only validated, this browser,
this browser with chat claims validated, or off. **Reply key for new requests** selects X-Wing (default) or X25519.

## Gaps

- A chat from the host's **+** asks twice: in the request form and in **Send invitation**.
- A declined request waits until the sender cancels it.
- No receipts. A sent message never shows delivered or read.
- Text over the network has not been tried between two hosts yet.
