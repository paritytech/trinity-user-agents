# Slide demo chat product

A sample chat product for the "slide to confirm" renderer node. Its worker opens the room
`slide-demo`, posts an intro text and a `register-card` message, and draws that card as a
`SlideToConfirm` ("Slide to register"). Completing the slide shows a loading state for 1.5 s, then
"Registered", and posts a confirmation text. Chat commands: `!reset` posts a fresh card, `!echo <x>`
replies `Echo: <x>`.

It talks to the host through the core's own client, `@parity/truapi`, taken from this checkout via
a `file:` dependency, so the client must be generated and built first (it needs `SlideToConfirm`):

```sh
./scripts/codegen.sh                                   # from the repo root
( cd js/packages/truapi && npm install && npm run build )
```

## Build

```sh
cd hosts/android/feature/products/product-sample/slide-chat
npm install
npm run build        # out/worker/index.js, out/index.html
npm run typecheck
```

## Run on the Android emulator

From the repo root, with the APK already built (skips the diagnosis step and the product build):

```sh
TRUAPI_ANDROID_E2E_CHAT_PRODUCT_DIR=hosts/android/feature/products/product-sample/slide-chat TRUAPI_ANDROID_E2E_CHAT_PRODUCT_HOST=slide-demo.dot TRUAPI_ANDROID_E2E_CHAT_PRODUCT_NAME="Slide demo" TRUAPI_ANDROID_E2E_CHAT_ROOM_ID=slide-demo TRUAPI_ANDROID_E2E_CHAT_DIAGNOSIS=0 TRUAPI_ANDROID_E2E_CHAT_MESSAGE="!echo hello" TRUAPI_ANDROID_E2E_CHAT_EXPECTED_REPLY="Echo: hello" TRUAPI_ANDROID_E2E_CHAT_EXPECTED_STARTUP_MESSAGE="Tonight's game is open" TRUAPI_ANDROID_E2E_SKIP_PRODUCT_BUILD=1 node scripts/launch-android-chat-playground.mjs
```

The launcher serves `out/` at `http://127.0.0.1:3100` and the host loads `out/worker/index.js`.
`PRODUCT_NAME` must appear in `index.html` (the launcher uses it to recognise its own server), which
is why the page title is "Slide demo".
