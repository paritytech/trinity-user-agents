# Open a product from a dev server on a phone

A debug build of either app opens a product straight from a development server
on your Mac, so a change shows up on the phone without a deploy.

## Start the dev server

Serve the product over plain `http` on every interface. `next dev` does this by
default. Vite needs `--host`.

Run the dev server on its own, not under `truapi-host dev`, so the app is the
only host the page talks to.

On a physical phone you need the Mac's LAN address:

```bash
ipconfig getifaddr en0
```

## Open it

On iOS, shake the app and choose **Debug Settings → Open dev server**. It is
there in the Debug and DevCI configurations. On Android, open the debug menu
and choose **Open dev server**. It is there in the debug build type.

| Device | Address to enter |
| --- | --- |
| iOS Simulator | `localhost:3000` |
| iPhone on the same Wi-Fi | `192.168.x.x:3000`, and allow Local Network access the first time |
| Android emulator | `10.0.2.2:3000` |
| Android phone over USB or wireless debugging | `localhost:3000` after `adb reverse tcp:3000 tcp:3000` |
| Android phone on the same Wi-Fi | `192.168.x.x:3000` |

The app accepts `localhost` and loopback or private IPv4 addresses over `http`,
and refuses anything else.

## What the product sees

- It runs under `localhost:<port>`, the identifier dotli gives a local product,
  whichever address the phone reached the server by. Storage, permissions and
  account derivation key off that identifier, so two servers on the same port
  are the same product to the app.
- It always runs on the TrUAPI runtime, whichever runtime the debug toggle
  selects for dotNS products.
- A `localhost` product can sign as any other product's account without asking.
  That is why only development builds open one.

## Reloading

Hot reload goes over a WebSocket, which the app asks you to allow the first
time. To reload the whole page, use **More → Refresh** on iOS or the refresh
button in the bottom corner on Android.
