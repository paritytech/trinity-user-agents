---
"@parity/truapi": major
---

`truapi-host` takes every network from a stack descriptor: a built-in one through `--network`, any other through `--stack` or `HOST_CLI_STACK`. A fork or a local network is a `--stack` descriptor, for example a local file whose `rpc` and `dub` URLs are `ws://` and `http://` on loopback.

- `HOST_CLI_PEOPLE_WS`, `HOST_CLI_ASSET_HUB_WS`, `HOST_CLI_BULLETIN_WS`, `HOST_CLI_IDENTITY_BACKEND_BASE`, `HOST_CLI_DOTNS_POP_CONTROLLER` and `HOST_CLI_LOCAL_NETWORK_CONFIG` are removed and no longer read.
- The hidden `local-network-check` command is removed.
- A stack without a `dub` service or a `dotns.popController` is refused for every command.
- Usernames resolve through the stack's `dotns.popController`; the CLI no longer looks the controller up on chain.
- An endpoint of a local stack that does not answer within 3 seconds is logged and left to fail on first use, as for any other stack.
