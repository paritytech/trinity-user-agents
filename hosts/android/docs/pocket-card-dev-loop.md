# Building a Pocket card without deploying

Two loops for working on a Pocket card against a debug build of the app. Neither writes to a chain,
and neither needs a published manifest.

Both need a **debug build** (`assembleGpDebug` or `assembleVanillaDebug`, or the Firebase debug
distribution). They are off entirely on release builds.

## Where this works

Anywhere `adb` can reach the app: an emulator, or a physical device over USB or Wi-Fi debugging.
Nothing below is emulator-specific.

What it does **not** cover is a phone that only has the app from Firebase App Distribution, with no
`adb` connection. The tunnel below is what makes your dev server reachable, and there is no way to
set it up without `adb`. Until a browser-based preview exists, the fast loop needs a cable.

> **On the emulator, do not use `10.0.2.2`.** It is the usual alias for the host machine, but the
> app permits cleartext to loopback only, so a face served over `http://10.0.2.2:5173` is refused
> before it is fetched. Use `adb reverse` and `127.0.0.1` on the emulator exactly as on a phone.

## Before either loop

Serve your files from your machine and make them reachable from the device:

```sh
# anything that serves static files will do
npx serve -l 5173 .

# point the device's own loopback at your machine
adb reverse tcp:5173 tcp:5173
```

`127.0.0.1` is already permitted for cleartext in the app's network security config, so plain HTTP
over this tunnel works with no further setup. Any other host needs HTTPS.

A face file is a renderer tree in the generated TypeScript shape — `{ tag, value }` per variant,
PascalCase enum names. Two encoding rules catch most first attempts:

- **A `Padding` or `Margin` needs `top` and `end`.** They are a shorthand: `bottom` defaults to `top`
  and `start` defaults to `end`, so `{ "top": 16, "end": 16 }` is 16 all round. Omitting `end` is
  rejected as a missing field.
- **Sizes are non-negative whole numbers.** `16.5` is refused, not rounded.

The smallest face that draws:

```json
{
  "tag": "Column",
  "value": {
    "modifiers": [{ "tag": "Padding", "value": { "top": 16, "end": 16 } }],
    "props": {},
    "children": [
      {
        "tag": "Text",
        "value": {
          "modifiers": [],
          "props": { "style": "TitleMediumRegular", "color": "FgPrimary" },
          "children": [{ "tag": "String", "value": { "text": "Loyalty" } }]
        }
      }
    ]
  }
}
```

## Loop A — how does the face look

Answers the visual question, with no worker, no manifest and no product.

1. Open the app's debug menu and choose **Pocket face preview**.
2. Enter your face's URL, for example `http://127.0.0.1:5173/faces/loyalty.json`, and press **Draw**.
3. Edit the file on your machine, press **Draw** again.

What it proves: the face decodes, the tokens resolve, the text fits, and the layout holds at the real
card frame in both themes. It draws through the same decoder and the same renderer the Pocket tab
uses, so it cannot agree with a face the app would not draw.

Two things it does not show. The frame is the one the **approval sheet** gives a card — the Pocket
tab additionally lays the card on its own ground with a grain texture. And images do not resolve
here: an `Image` node names a Bulletin CID or a path inside a worker archive, and a loose file on a
dev server has neither, so it draws as empty space.

If the face is rejected, the message under the frame is the decoder's own, naming what it refused.

## Loop B — the whole card, live

Runs your real worker and streams a live face into a real card, redrawing in place.

**1. Build and serve the worker.** The bundle and the static faces come out of the same `dist`:

```sh
npm run build:worker
npx serve -l 5173 packages/chat-worker/dist
adb reverse tcp:5173 tcp:5173
```

Check the tunnel with Loop A before going further: draw
`http://127.0.0.1:5173/pocket/devicehood.json` in the face preview. If that works, the server, the
tunnel and the face are all good, and anything that fails after this is the card path.

**2. Point a product at it.** Debug menu → **Product bots** → add or edit, on a dotNS name with **no
published worker**:

| Field | Value |
| :-- | :-- |
| dotNS name | a name on your current TLD with no worker record |
| Script URL | `http://127.0.0.1:5173/index.js` |
| Pocket card id | the id your worker answers `onRender` for, e.g. `loyalty` |
| Pocket card title | what the approval sheet calls the card |
| Pocket card face URL | `http://127.0.0.1:5173/faces/loyalty.json` |
| App URL | optional, `http://127.0.0.1:<port>/...` only; the page the card opens over, in place of the product's published app |
| Open with the face away | off by default; on, the card opens with its face out of the way |

Two traps in this form:

- **The TLD is appended, not validated.** A name that does not already end with the current TLD
  suffix gets it added, so `myproduct.testnet` on a `.test` host silently becomes
  `myproduct.testnet.test`. The product row in the list shows the id it actually used — read it there
  before building a deeplink.
- **Re-confirming the form is what applies a change.** The card rides on the resolved worker, and
  saving is what invalidates that resolution.

**3. Open the add link.** "Following the link" means getting Android to open that URL:

```sh
adb shell am start -n io.parity.polkadotapp.debug/io.paritytech.polkadotapp.app.root.presentation.root.RootActivity \
  -a android.intent.action.VIEW \
  -d "'polkadotapp://myproduct.paseo/-/pocket/add?card=loyalty'"
```

Use the id the **Product bots list** shows on the row, not what you typed into the form.

The URL is quoted twice on purpose. The command crosses two shells — yours, and the device's — and
the device's reads `?` as a glob and `<`/`>` as redirects. Wrapping this in a one-line script in
your own repo is worth it if you run it often.

Naming the activity with `-n` makes this independent of the manifest's intent filters, so it works
even on a TLD the app registers no filter for. Without `-n` the host must match one of `*.dot`,
`*.paseo`, `*.test`, `*.testnet`, and `Error: Activity not started, unable to resolve Intent` means
none did.

**4. Approve the sheet.** It draws the *static* face from your face URL, because no worker runs until
a card exists. Approving adds the card to the Pocket tab.

**5. Watch it redraw.** From here the face comes from the worker's `renderer.onRender`, not the URL.
The host holds that subscription open for as long as the card is on screen, so the worker can send a
new tree whenever it likes. A worker that redraws on a short timer — a counter, a clock — makes a
live card obvious at a glance while you are working on one.

What this proves: the manifest shape, the add flow, the live render stream, redraws in place, and
the host releasing the render when the card leaves the screen.

### Things worth knowing

- **A published worker always wins.** If the product's dotNS record has a worker, that is what runs;
  the debug URL only fills in when there is none. Use a name with no published worker while
  iterating.
- **Changing the card means re-confirming the form.** The card rides on the resolved worker, and
  confirming is what invalidates that resolution.
- **A rejected card id reads as no card.** Ids are screened with the rules the core applies, so a bad
  one leaves the worker with no card rather than failing later. Check logcat for `pocket:` if a card
  you named does not appear.
- **Faces are capped at 256 KiB**, wherever they are served from.
- **The worker bundle is cached by the WebView, and the worker is kept warm.** Rebuilding and
  switching tabs is not enough to pick up a change: the same worker process keeps running, and even a
  fresh one can be served the old bundle from cache. Serve with `Cache-Control: no-store` and
  `adb shell am force-stop <package>` before retesting, or you will be reading results from the code
  you replaced.

## The expanded card, live

An opened card shows its product's page under the face, and that page can fold the face away and
back. To try it with the sample worker in `feature/products/product-sample/pocket-worker` (its README
has the details):

1. Turn **TrUAPI runtime (products)** on in the debug menu.
2. Build the sample and serve its `dist/` on port 5173, then `adb reverse tcp:5173 tcp:5173`.
3. In the product form set Script URL `http://127.0.0.1:5173/worker.js`, face URL
   `http://127.0.0.1:5173/faces/loyalty.json` and App URL `http://127.0.0.1:5173/index.html`. Add
   `?hideOnLoad` to the App URL to test a call made as the page loads.
4. Add the card and open it.

The page has buttons that call `truapi.expandedCard.setFaceShown({ shown })` and a log of each
answer, plus its own height and a red bar pinned to its bottom edge. The call answers `UserMoving`
while the user drags the face (and until it has settled) and `NotPresented` when the card is not open.

The WebView keeps the last card's page loaded, so restart the app after changing the App URL.

## When you are ready to publish

Declare the card in the worker's manifest instead, and the debug fields stop being involved:

```json
{
  "$v": 1,
  "kind": "worker",
  "appVersion": [1, 0, 0],
  "entrypoint": "worker.js",
  "includes": { "chat": false, "pocket": true },
  "pocket": {
    "cards": [{ "id": "loyalty", "title": "Loyalty", "preview": "faces/loyalty.json", "faceShown": false }]
  }
}
```

`faceShown` is optional and defaults to `true`; `false` opens the card with its face out of the way.

`preview` is now a path **inside the worker archive**, not a URL — a published manifest cannot name a
URL, by design. Publish with `bulletin-deploy`, which writes this as the `executable` record on
`worker.<product>.<tld>`.

Note that `bulletin-deploy` does not yet validate `pocket.cards`. A malformed card publishes cleanly
and is then dropped by the app with only a log line, so check the card appears after publishing.

### Pinned cards

A pinned card is one the host places in the Pocket itself, backed by a reserved product id and drawn
from a face bundled in the APK. It hands over to the product as soon as that product publishes a
worker card with the matching id: the host then boots the worker and streams its face in place of the
bundled one.

Until that record exists the card keeps the bundled face, and nothing on the device says why — so if
a pinned card does not pick up your worker, check the card id in the manifest first.
