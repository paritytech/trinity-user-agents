# The dock and Pocket

## Destinations

The dock holds **Chat**, **Pocket**, **Browse** and **Apps**, like a phone host's tab bar.

- A tab starts in Chat, or reopens the product that it had in front before a reload.
- Chat and Pocket are host screens. See [Chat](chat.md) and [Pocket](#pocket).
- **Browse** with nothing in front shows a host landing:
  - **Browse apps** opens the network's Browse app (`browse.paseo` on Paseo Next v2). A network with none has no row.
  - **Recent** lists the last five products that this wallet opened on this network.
  - A product opened while Browse is selected opens in Browse. **Close** brings the landing back.
- An address typed over Chat or Pocket opens as its own app.
- The Chat SPA is a product: open `chat-spa-verbosity.paseo`.
- On a dev server, `truapi-web-signing-host:dock:v2` (per network) can override Browse's address for one browser. A
  build ignores it. The host never writes it, and ignores the old `v1` key.

## The Docklet

- A glass dock, at most 440 px wide, after the Polkadot app's R1 Docklet: Chat and Pocket left of the Pill, Browse and
  Apps right of it, and a lens on the current tab. Apps shows the open-product count.
- Host screens and product frames end above the Docklet. A product gets no inset, so nothing of it is under the glass.
- The Pill opens the [search card](developer-tools.md#search-field).
- On iOS Safari, a keyboard only shrinks the visible area. So the page follows `visualViewport`
  (`src/app/keyboard-viewport.ts`) and stays above the keyboard. Pinch zoom and browsers that resize the layout are left
  alone.

## The Apps shelf

Apps opens the shelf (after the app's TabBarShelfView).

- Top row: the Inbox (when offered), the core's state for this network (green dot when ready, red when failed), and
  **Settings** (the Account tab).
- Below: the open products as round icons. The product in front is marked. Choose one to bring it in front. A long
  press, or **×**, closes it. Each icon counts its unseen Inbox items.
- Escape or a tap outside closes the shelf.

## An open product

- Two glass circles sit in a band over the product, as in the iOS browser tab (`SPAViewLayout`). The band is the top
  inset (54 px on a device), 12 px, the 44 px circles and 12 px. The frame starts below it and gets no inset.
- The band does not fold on scroll. The host cannot see a product's scrolling.
- **Minimize** (left) keeps an App running in the shelf. A Pocket Widget cannot run behind and gets **Close** instead.
- **More** (right) has **Product details** and **Close**.

## Products running behind

- Another destination hides the product in front but keeps its frame, core channel and page state. A green dot marks a
  destination whose product runs.
- A hidden product can still raise a permission prompt. The prompt names it.
- A reload or a session change (sign-in, sign-out, wallet switch, first username sync) ends every held product. Only the
  product in front opens again.
- A product's `navigateTo` for a name of this network (`https://`, `polkadot://` or `polkadotapp://`) opens it in front.
  The opener keeps running. A product that is already open comes to the front unchanged, as on iOS. Other `http(s)`
  addresses go to the browser. The host refuses anything else.

## Pocket

The host draws Pocket as the Android `PocketScreen`: a title over the digital dollar card and the Humanity card. The
host reads and signs nothing for it.

- **Digital dollar.** The host has no Coinage engine. The amount reads "Unavailable". `payment.balanceSubscribe` answers
  `Unsupported`.
- **Not here:** tilt shine, collectibles, adding and removing cards.
- **Licences.** The two card drawables come from the GPL-3.0 Polkadot Android app and keep that licence. The Inter and
  Manrope subsets use the SIL Open Font License 1.1. Notices are in `src/assets/` and ship under `licenses/`.

### Humanity card

The card follows the Pocket RFC (`docs/rfcs/pocket-modality.md`). The personhood product `peopl.<tld>` backs it. Its
face is what that product draws. It is not a personhood check.

- **Face.** The first time Pocket shows in a session, the host reads `worker.peopl.<tld>`. It runs the worker only when
  it serves Pocket (`includes.pocket`) with the `humanity` card. It asks for
  `RenderContext::PocketCard { cardId: "humanity" }`. Each tree replaces the face. Presses go back as renderer actions.
  Until then, the bundled face shows.
- **Retries.** Every 250 ms for 10 s while the worker boots. Then a dropped stream reopens after 1 s, doubling to 30 s.
- **Kept face.** Per network and wallet in `truapi-web-signing-host:network:<id>:pocket-faces:<wallet>:v1`, at most
  every 2 s. The next session shows it at once. A face over 256 KiB is not kept.
- **Lifetime.** From the first Pocket view to the end of the session. The native hosts start it with the session. Chat
  shares the same worker.
- **Widget.** A press opens the product's Widget at `/?card=humanity`, selected as on iOS (`ProductResolver`):
  1. An invalid root `manifest` text record on the base name: nothing opens, and Pocket says why. A valid one has `$v`
     1, a `displayName`, a `description`, and an icon with a `cid` and a `format`.
  2. With a valid root manifest, a valid `widget.peopl.<tld>` manifest (`$v` 1, `kind` `widget`, a `dimensions.height`)
     serves its `contenthash`. Unlike iOS, an invalid widget manifest is refused, not skipped.
  3. Otherwise the base name's `contenthash` runs as the Widget. `app.peopl.<tld>` is never read. On Paseo,
     `peopl.paseo` has no widget.
  4. No content: nothing opens, and Pocket says why.
- The Widget runs as a `Widget` execution of `peopl.<tld>`, sharing its storage, accounts and grants. Choosing Pocket
  ends it. A session change reopens it. A reload does not, and history does not record it.
- **Core calls.** `pocket.listSubscribe` shows `humanity` as privileged to `peopl.<tld>` only. `pocket.removeCard`
  refuses a pinned card, reported as `Unknown` with the reason.
- **Not here.** The face above an open Widget, so `expandedCard.setFaceShown` answers `Unsupported`. The Widget is not
  kept loaded after it closes.
