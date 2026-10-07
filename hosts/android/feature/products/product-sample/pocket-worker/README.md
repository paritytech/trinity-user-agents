# Pocket demo worker

A worker that publishes one Pocket card and draws its face live. It talks to the host through the
core's own client (`@parity/truapi`), so it needs no product SDK.

## Build

```sh
npm install
npm run build        # dist/: worker.js, widget.js, index.html, faces/
```

The published `@parity/truapi` (0.16.0) predates the Pocket API, so `client.pocket` is undefined
there and `npm run typecheck` fails against it. Until a release ships Pocket, build the client from
the core checkout that `truapi_ref` pins and install it over the published one:

```sh
( cd "$TRUAPI_DIR" && ./scripts/codegen.sh )
( cd "$TRUAPI_DIR/js/packages/truapi" && npm install && npm run build )
npm install --no-save "$TRUAPI_DIR/js/packages/truapi"
```

## Publish

The worker archive is what the host reads, so publish these files under the product's
`worker.<name>.<tld>` dotNS name:

```
worker.js            <- dist/worker.js
faces/loyalty.json   <- the static face the approval sheet shows
```

and set that name's `executable` text record to `manifest/worker.json`. The root manifest of
`<name>.<tld>` is unchanged.

## Try it on Android

1. Open `polkadotapp://<name>.<tld>/-/pocket/add?card=loyalty`. The approval sheet shows
   `faces/loyalty.json`; Add puts the card in the Pocket.
2. With the card on screen the host starts this worker, opens a render for `PocketCard { loyalty }`,
   and the face switches to the live tree. Stamp increments the counter; Remove asks the host to
   drop the card, which ends the render and stops the worker.
3. The console logs the card list on every change (`Pocket demo: cards ...`).

## Try the expanded card

`widget/index.html` is the page the host shows below the face when the card is opened. It has
buttons that call `expandedCard.setFaceShown` (Hide face, Show face, Hide face in 2 s), a log with
each call's answer (`ok`, `UserMoving`, `NotPresented`, `Unsupported`, `Denied`), the page's
`innerHeight` (updated on resize) and a red BOTTOM EDGE bar pinned to the bottom of the page, which
goes missing when the host sizes the page wrong. The card's `faceShown` in `manifest/worker.json`
sets whether the face starts shown.

1. `npm run build`, then serve `dist/` on port 5173, for example `npx serve -l 5173 dist`.
2. `adb reverse tcp:5173 tcp:5173` so the emulator reaches it.
3. Point the debug product's App URL at `http://127.0.0.1:5173/index.html`, add the card and open it.
   Add `?hideOnLoad` to the URL to hide the face as soon as the page loads.
