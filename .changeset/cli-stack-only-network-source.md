---
"@parity/truapi": major
---

`truapi-host` takes every network from a stack descriptor: a built-in one through `--network`, any other through `--stack` or `HOST_CLI_STACK`. A fork or a local network is a `--stack` descriptor, for example a local file whose `rpc` and `dub` URLs are `ws://` and `http://` on loopback.

- `HOST_CLI_PEOPLE_WS`, `HOST_CLI_ASSET_HUB_WS`, `HOST_CLI_BULLETIN_WS`, `HOST_CLI_IDENTITY_BACKEND_BASE`, `HOST_CLI_DOTNS_POP_CONTROLLER` and `HOST_CLI_LOCAL_NETWORK_CONFIG` are removed. Setting one to a non-blank value stops every command with an error that points at `--stack`.
- The hidden `local-network-check` command is removed.
- A stack without a `dub` service or a `dotns.popController` is refused for every command.
- The CLI's own username reads go through the stack's `dotns.popController` and no longer look the controller up on chain. The signing host's core still discovers its controller on chain.
- A published built-in descriptor that changes the stack's `id`, or that the CLI cannot run on, is not used: the bundled copy stays in use with a warning, as for a `tld` change.
- `truapi-host stack bundle` refuses a built-in network's copy that the CLI cannot run on, and a copy that changes the `id` of the one at `--out`.
- An endpoint of a local stack that does not answer within 3 seconds is logged and left to fail on first use, as for any other stack.
