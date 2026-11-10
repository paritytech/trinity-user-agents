# Inspector

The Inspector tab has six views. Each keeps its data in memory, in this tab. None records keys, recovery phrases or
secrets.

## Activity

What the host did, oldest first, one row per event (`tools/activity/`, fed by `src/platform/activity.ts`).

- A row has a time, a category (Session, Products, Workers, Network, Host), what happened, and a red reason on failure.
  Enter or a click opens its details.
- Filters: one category, **Errors**, and a search over all text. **Pause** keeps logging but stops following.
- The log keeps the last 500 events. It does not store or send them.
- Events hold names, product ids, addresses and reasons. A URL loses its user name, password and secret-looking query
  parameters.
- Network events are the host's own, such as each chain's backend when the network connects. Traffic is in TrUAPI.

## TrUAPI

- `createInAppDebugger` from `@parity/truapi-debugger` groups and decodes the frames that the page relays between
  product and core.
- If the core's wire schema differs from the page bundle's, the view lists operations without decoding them.
- Frames may carry sensitive payloads.

## Workers

- Lists the product workers that the session wants or runs, including host and Pocket workers, with their state: not
  started, starting, running, or failed with the reason.
- **Traces** opens the TrUAPI view filtered to the worker's channel.
- **Restart** shows only for a user-added worker that runs for Chat alone. **Remove** is in Product → Workers.
- **Console:** what the worker logged, its uncaught errors and unhandled rejections.
  - `truapi-sandbox/console.js` (`src/products/loader/console.ts`) loads before the entry, so it sees the first line. It
    calls the original methods, so the worker and the browser see no change.
  - Each argument is bounded (`describe.ts`): 3 levels, 20 entries, 500 characters per string, 4,000 per line, no getter
    runs, binary by size only, loops as `[Circular]`. At most 50 lines and 50 errors a second. The rest are counted.
  - The host accepts lines only from its own worker frames, bounds them again, and strips credentials from URLs
    (`src/products/worker-console.ts`). It keeps 200 lines per worker, labelled by run, until removal or session end.
  - The lines are the worker's own text. The host does not clean them.

## Statements

What this tab saw of the People Statement Store (`tools/statements/`), to debug a chat protocol and the resources that
it spends. It names no chat protocol.

### Sources

Every fact has a source tag:

- **People node** or **light client**: the running People backend's answers and pages;
- **core**: claim results. **host**: the request service's records. **chain**: a read at a block;
- **reported**: a product's own claim, unchecked. **derived**: worked out here. **inspector**: the view's own events.

- `src/platform/statement-observer.ts` wraps every People connection. It reads only `statement_*` traffic, and passes
  every message on unchanged, in order, with no delay.
- The request service and network delivery report facts (`src/invitations/observer.ts`). Workers may report
  conversations (`bridges/diagnostics.ts`).

### Transport

`tools/statements/transport.ts` sets the wording by People's running backend. The evidence names it (`peopleBackend`).

| Fact                    | People node (default)                                   | Light client (smoldot-light 2.1.0)                         |
| ----------------------- | ------------------------------------------------------- | ---------------------------------------------------------- |
| Store                   | The node keeps one. The host trusts its answers.        | None.                                                      |
| `new` on submit         | **Accepted** into the node's store                      | **Broadcast**: one peer took it, not shown stored          |
| Refusals                | The node's own show with a reason. Other nodes' do not. | Peers' refusals never show. `invalid` or RPC error 7001 do |
| First subscription page | Replays what the node holds                             | Always empty, `remaining: 0`                               |
| Later pages             | What reaches the node                                   | What peers send, new or old                                |

- The view opens no subscription of its own. On the light client, all People connections share one topic filter, and a
  new subscription would replace the chat's topics (smoldot-light `background.rs:922-929`).
- No refusal shown does not mean that none happened.

### Tabs

- **Accounts:** one row per account claimed, reported or seen signing. Columns:
  - **Role**: Chat sender or peer (reported), Setup notification, Product account, or Observed signer;
  - **Owner** (below) and **Allocation** (the latest slot claim);
  - **Grant**: statements · size, from **Read grants** at the newest finalized block;
  - **Observed**: the latest statements that this tab saw.
- **Owner** records where the account came from in this tab, not a signature check: **This wallet** (derived here this
  session), **Reported only**, or **Unknown here**. Unknown does not mean "not this wallet".
- An account's detail has a **Chat** line, a link to the Chat view's card, for each chat it is tied to:
  - **host:** the host let it post for a request (`chatAccount`). The card is the conversation its product reports for
    that request, or the request on its own.
  - **reported:** a product's report names it as its account or the peer in a conversation. The host did not check it.
  - An account tied to more than one chat has one line per chat. A notification account, or an account no one tied, has
    none.
- An account's **Channels** list its channels, with the latest observed statement and how many it replaced. Choosing one
  narrows the account's statements to it, each marked **latest** or **replaced**. The chip under the list, choosing the
  account again, or a link to the account alone shows every channel again. A channel belongs to its account: other
  accounts' statements on the same channel id never show under it.
- An account's or a topic's statements list the last 20 observed, the last observed first. A statement seen again moves
  to the top, once. Each row shows when it was last observed, and how many times since it was first observed.
- **Topics:** one row per topic. Names are given only when known: the host's day inbox
  (`Shared setup inbox · today UTC`, derived) or a name from a product report (reported). **Role**: Listening,
  Publishing (accepted or broadcast), Both, Tried (none known to have gone out), or Other observed.
- **Timeline:** every fact in order, with filters.

### What it cannot know

- **Observed** is the latest statement seen per channel, not occupancy. It is no bound on what the network holds. The
  view never compares it with the grant. None observed does not mean none stored.
- A grant is not what is left. A validated claim allocated nothing. Remaining daily slots are not readable.
- A refusal by this tab's no-submit guard counts as tried here, not as a node or peer answer.

### Limits

- Kept in memory: 800 events, 1,000 statements, 400 submissions, 200 subscriptions, 300 accounts and topics, 600
  channels, 64 conversations, 200 requests. The view counts what it drops. Statements go by last observation: the one
  observed longest ago goes first.
- The observer keeps at most 256 unanswered requests and subscriptions per connection.
- **Capture data** keeps statement bytes: 16 KiB each, 512 KiB in all. **Copy evidence** copies metadata only, as JSON.
- Text from the node, light client or core has long hex replaced by its length and is cut at 200 characters.
- The view starts empty for each session and on **Clear**. Connections that outlive a session stay, marked as earlier.

## Chat

Conversations that product workers report about themselves (`tools/chat/`), and host requests with no report.

- A card shows the protocol, the host request and its `chatAccount` answer, the account (reported), a **tie** line,
  peers, topics, message counts and the statements per message.
- **Tie:** "tied to" the product only when a claim or allowance of that product names the account. Else "reported only".
- **Protocol details** are what only the protocol defines, as reported.
- **Statements** (derived): per account the card names (own, the host's allowance when another, peers), the latest
  observed statements per channel, the channels and the last time. Each opens the account in Statements. **Details** has
  one **Channel** link per account and channel, which opens the account narrowed to that channel; the channel the
  product says it posts on is marked reported, or listed as not observed. The accounts are the product's word unless the
  Tie line says otherwise. It is this tab's observed traffic, not the network's.
- The host reads no chat payload. Account and topic links open the Statements view. A stopped worker's card is stale.

## Products

One row per mounted load, newest first (`tools/products/`): products by name, Pocket Widgets and product workers.

- A row shows size, files, blocks, time, transport (CAR, CAR+raw, raw or cache; `· host` or `· seed` for blocks from
  this origin) and outcome. A block that came both ways counts twice.
- An open row shows the record that answered, the content id, the gateway and the lookup time, then the loader's counts:
  phases, files, the CAR stream, the hedge (the 2-second wait before raw block requests), and the block requests. Each
  fact is tagged host, loader or derived.
- It reads no logs. A load whose frame closed before its report shows closed.
- At most 50 loads. A new session drops closed loads.
