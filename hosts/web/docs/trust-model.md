# Trust model

This host is a development tool. It trusts its user and the products that the user opens. It is not a sandbox. It makes
no promise against a hostile product. Its separations keep development data tidy. They are not security boundaries.

## Rules

- **Wallets are disposable.** Recovery phrases are unencrypted in `localStorage` on the host's origin. Any script on
  that origin can read them. Never import a phrase that holds value.
- **A product opened by name runs on the host's origin.** The host is one static site with one origin. The product frame
  can read the saved wallets through `window.parent`. Open only products that you wrote or trust.
- **A product opened by URL stays on its own origin.** The host refuses to open its own origin by URL.
- **Separation by wallet is by name.** Storage keys hold the wallet and product id. The browser does not isolate them.
  See [Storage](wallets-and-sessions.md#storage).
- **Prompts are a development aid, not custody.** See [Prompts](#prompts).
- **A product's own requests follow the browser.** The host does not intercept `fetch`, XHR, WebSocket, WebRTC or media.
  Only TrUAPI calls reach the core.
- **Anyone who can reach a served host can use it.** Serve it on loopback, or treat the page as public. Wallets stay in
  each visitor's own browser.

## Prompts

- Every host prompt uses the Polkadot app's bottom sheet, over the app's rectangle only. The developer tools stay
  visible.
- The sheet names the requesting product once: "Requested by" and the full product id that the core holds.
- When the id does not say where the product came from, the host adds a line, for example
  `App opened from http://localhost:5173/`.
- When two open products share one id, the core and the host cannot tell them apart. The sheet says so.
- A product cannot change these lines. A prompt about a host action names no product.
- Escape or a tap outside gives the safe answer: deny, reject or cancel.

## Differences from the native hosts

- The native hosts load `js/container`, which asks the core before a page's own network or media request. This host does
  not.
- Plain HTTP has no secure-context APIs. On `http://<LAN address>`, camera and microphone calls throw before any prompt.
- Native-only APIs are not emulated.
