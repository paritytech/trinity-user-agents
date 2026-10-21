---
"@parity/truapi": minor
---

The CLI host takes `HOST_CLI_PEOPLE_WS`, `HOST_CLI_ASSET_HUB_WS` and `HOST_CLI_BULLETIN_WS` to point a network preset at a fork of that network. They must be `ws://` or `wss://` URLs and cannot be combined with `TRUAPI_LIGHT_CLIENT=1`. `truapi-host attest` attests a given mnemonic's identity, waits for its ring membership, and prints `{address, publicKey, liteUsername}` as JSON; against a fork it also needs `HOST_CLI_IDENTITY_BACKEND_BASE`.
