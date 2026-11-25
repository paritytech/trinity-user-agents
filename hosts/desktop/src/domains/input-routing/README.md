# input-routing

Takes arbitrary user input — typed text, a picked file, a scanned code, an OS-delivered link — resolves it into one
machine-readable value, and decides who receives it. Input is **not a modality**: the surface opens over another one,
and the screen underneath is what names the recipients. Implements the host side of **RFC-0027 (Input Routing)**.

The domain owns the _routing_ question only. Capturing input (a text field, a camera, a URL handler) is a host-local
concern that lives in the feature; opening a product surface is `product`'s job. This domain sits between them.

## Vocabulary

### The input

- **Routed input** — what the host resolved an input to. Either **open this surface of this product** — `app` (a path),
  `chat` (a room), `pocket` (an artifact) — or **here is a query, who can answer it** (`query`). One type carries both.
  The first three carry their own context and replace the screen; `query` carries none and inherits the screen's.
- **Query** — an input that named no product, so a recipient has to be found. Either `text` or an `attachment`.
- **Attachment** — a user-supplied payload carried whole, categorized as `image`, `audio`, `video`, or `file`. The
  **category is derived** from `mimeType` by the host and carries no privilege — it says how the payload is _labelled_,
  not what the bytes are. An attachment with no type from the OS is labelled `application/octet-stream` and lands in
  `file`, never guessed at.
- **Input source** — `user`, `scanned`, or `operating-system`. The split that matters is **externally authored**
  (`scanned`, `operating-system`) versus **user-originated** (`user`), and any branch on provenance is a branch on that
  split rather than on `scanned` alone. A link fired through the OS handler was authored by whatever page the user came
  from, so treating it as safer than a printed code leaves a hole the size of a hostile web page. Nothing branches on it
  today — the source is carried end to end and handed to the worker, which is where the decision belongs.
- **Round** — one fan-out of a query to the context set, and the merged answers it produces. User-composed text starts a
  round on a **pause in typing**, never on a keystroke and never on a submit; each round supersedes the last, so what
  the user sees answers the string currently in the field.
- **Confirmation** — the gate on **externally authored** input only. A scanned code or an OS-delivered payload is shown
  to the user, with what the host will do about it, and routes only on acceptance. Confirming says the host may
  proceed; it never makes the user the author, so provenance survives the gate unchanged.

### The recipients

- **Query declaration** — a worker's statement of the query _shapes_ it accepts: `text` (boolean) and `attachment`
  (a set of the four categories). Eligibility is set membership, and an unrecognized member is ignored rather than
  rejecting the declaration. It names shapes, never content — there is deliberately no way to declare "I answer SS58 addresses".
  A declaration is a filter, not a promise: a declaring worker still answers `notHandled` for most queries, and that is
  the expected steady state.
- **Query recipient** — a product's worker paired with its declaration.
- **Context set** — the products the screen underneath contributed, and the whole of who is asked. A text query goes to
  those of them that declared `text`, bounded by the context cap. **Nothing outside the set is ever asked.**
- **Picker** — an attachment goes to **exactly one** product the user names, chosen from those that declared its
  category. An attachment is **never broadcast**: it carries its contents, so there is no version of a broadcast that
  discloses less than everything.

### The answers

- **Input response** — `notHandled` (the ordinary answer, not an error) or `candidates`.
- **Candidate content** — `text`, `richText`, `file`, or `custom`. A candidate is **content and nothing else**: no
  title, no URL of its own, no icon, no identity. The host frames it with the answering product's manifest identity,
  which is what makes candidate spoofing structurally impossible.
- **Ranked candidate** — a candidate merged into the host list, attributed to the product that answered.
- **Candidate node** — the render tree a product hands back for one of its `custom` candidates: the same closed
  vocabulary a chat custom message is drawn from (layouts, typography and colour tokens — never markup, styles or URLs).
  The host carries a `custom` candidate's bytes without interpreting them, then asks whoever wrote them what they should
  look like; `candidateId` is what a render call and an action both name. A tree is **live** — the product pushes a new
  one when its state changes — so drawing one is a subscription, not a read.

## Scope

**Owns:** resolving a string into a `RoutedInput`; deriving attachment categories; filtering recipients by declared
category; computing recipient sets from product ids it is given; running a query round under the RFC's bounds; merging
and ranking candidates; delivering a selection back to its answerer; asking a product to draw one of its `custom`
candidates.

**Does not own:** input capture, the scan confirmation UI, the candidate list UI, the attachment picker UI (all
feature); **deriving the context set from the screen** — that is host-UI knowledge, so the feature reads it and passes
the product ids down as a parameter; **drawing a candidate node** — the tree is carried here and rendered by
`@/widgets/RendererTree`, which chat shares; opening a product surface, product identity, installed-product
enumeration (all `product`).

## Flows

### Answering text

`routingUseCase.resolveInput(text)` → a `RoutedInput`. An `app` is handed to `product` to open. A `query` goes to
`textRecipientsFor(productIds)` and then `runQueryRound(...)`, which returns an `Observable<QueryRound>` emitting the
merged list as answers arrive and completing at the hard deadline. Abort the caller's signal to supersede the round.

### Answering an attachment

`routingUseCase.toAttachment(data)` derives the category. `exceedsAttachmentCap(size)` declines an oversized one to the
user before anything moves. `attachmentRecipientsFor(productIds, category)` builds the picker; the user names one
product and it alone receives the bytes.

### Selecting a candidate

`routingUseCase.selectCandidate(...)` delivers `CandidateSelected` to the product that answered. Terminal — the
product's answer to being chosen is whatever it does next, through the APIs it already holds. Nothing a product
returned is interpreted by the host as an instruction.

## Boundaries

- Depends on `product` (for `dotNsService` name parsing). `product` must never depend on this domain.
- `gateway.ts` is the product-worker boundary and is **entirely mocked** — no TrUAPI connection, no worker launch, no
  bytes leaving the renderer. It is the single file the real `input_request` wiring replaces.
- The `query` key on the worker manifest is validated in `product`'s `manifest/schemas.ts`, where every other manifest
  field is. This domain consumes the parsed value; it does not re-validate it.

## Divergences from the RFC

Recorded so the gap is visible rather than discovered later:

- **The Widget context** (RFC Q1). The RFC says a host must define no context set for a dashboard rather than guess one,
  and that the surface does not open over one. This host defines it: every product added to the dashboard, across all
  pages, folder contents included. The RFC's first required property holds outright — the user assembled the set by
  placing each product, and a product never placed never appears. The second, "small and known in advance", holds per
  page rather than across them; `CONTEXT_CAP` is what bounds the whole.
- **Multi-product conversations** (RFC Q2). The chat view here lists rooms across many products, so its context set is
  every product the user has a room with. Chat is a fan-out context, like a pocket.
- **No input syntax** (RFC Q4). `resolveInput` recognizes only the `polkadot://` deeplink form this host already
  generates. No string currently resolves to `chat` or `pocket` — the variants are routable, but nothing produces them.
- **Ranking** (RFC Q5) is per-product quota plus round-robin interleave, preserving each product's own order within its
  slots. Chosen so a product that answers every query gains one row rather than the list.
- **Attachments take no answer** (RFC Q9 is open). The picker delivers; nothing comes back.
- **The extension cap is fixed at zero.** This host never queries a product outside the context set, so there is no
  outside band and no band boundary to enforce. The disclosure property is therefore the RFC's unconditional one: an
  input reaches only products the user can see on screen.
- **Soft deadline** is satisfied by superset: the round emits incrementally as answers arrive, rather than batching to
  500 ms and then merging.
- **Every product is mocked as declaring everything.** RFC-0027 § Registration says "a worker that says nothing is never
  asked anything", but the real read — `query` off the worker executable manifest — does not exist until the manifest
  RFC lands. Until then `gateway.ts` returns a permissive declaration for every id, which inverts the default and makes
  the declaration filter dead in practice. Harmless while the extension cap is zero, because the set asked is still
  exactly what is on screen; it becomes wrong the moment a host asks beyond the context, since the declaration is the
  whole of an outside product's consent.
- **No product worker answers anything; a DI seam can.** `input_request` is not on the wire, so
  `$usecase/routing.ts` asks `answerQueryTransformer` and hands the result to `gateway.ts`, which
  falls back to `notHandled` — which is what an unwired host should say, and what every product says
  when no handler is registered. The seam lives with the use case rather than beside the wire it
  stands in for because a handler is registered from outside the domain, and the public surface may
  not re-export `gateway.ts`; the wire therefore stays free of DI, taking the answer as a parameter
  that disappears with the real transport. The gateway
  itself never fabricates: it did once, with "Open «whatever you typed»" candidates that read as
  offers the host could act on, and selecting one logged a line and closed the surface. An invented
  answer that looks like a real action is worse than no answer, because the only way to discover it
  does nothing is to press it. Anything that does answer today registers a handler from outside the
  domain and is deletable in one line, which keeps the fabrication visible instead of resident.
- **A custom candidate is drawn through a second seam, for want of an SDK method.** RFC-0027 gives a
  custom candidate a `candidateId` precisely so a render call and an action can both name one drawn
  thing, but the product protocol exposes exactly one render entry —
  `renderChatCustomMessage` — and no generic equivalent. `renderCandidateTransformer` stands in,
  shaped like the subscription that will replace it (a callback plus a teardown), so the real method
  drops in without moving the seam. The action's return path has no stand-in at all: nothing can tell
  a product that a control inside its own tree was used. **This is an SDK gap, not a host decision.**
- **A candidate's media is fetched to draw the list.** RFC-0027 § Security says not to, and the reason is
  precise: the request tells the answering product's server that this user saw this candidate, before the
  user has chosen anything — so a product learns the outcome of a query it only speculatively answered.
  The host fetches anyway (`richText` media renders through a real `<img src>`), because a list of
  thumbnail-shaped holes is not worth shipping. **This is an accepted leak, not an oversight.** The fix
  when it matters is a host-side proxy, or fetching only once a candidate is taken — not a smaller
  thumbnail. The product **identity** icon beside a candidate is a different thing and is not covered by
  this: it comes from the manifest of a product the user already installed, is content-addressed, and is
  already cached elsewhere in the app, so drawing it discloses nothing the query did not.
- **A navigation opens the product.** `RoutedInput.app` is resolved here and the feature applies the `product`
  feature's own side effect to open it — this domain still never navigates, which is why the arrow points outward
  rather than into a router. Taking a typed identifier the user has never installed shows it first, resolved from
  its manifest into memory: naming a product is not installing one.

## References

- RFC-0027 (Input Routing) — `paritytech/trinity-user-agents`, `docs/rfcs/0027-input-routing.md`.
- RFC-0002 (Permission Model) — `OpenUrl`, which governs a selected product's own external navigation.
- Product Manifest Format RFC — `paritytech/triangle-js-sdks`, branch `rfc/product-manifest`, which owns
  `worker.<product_id>.dot` and absorbs the `query` key.
