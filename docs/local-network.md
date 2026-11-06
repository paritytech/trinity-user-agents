# Local-network routing for the CLI host

`HOST_CLI_LOCAL_NETWORK_CONFIG` points the CLI host at a disposable local copy of a
built-in test network. It runs under the product namespace of `--network paseo-next-v2`
(the default), `--network previewnet` or `--network pcf-devnet`, and replaces all chain
endpoints, genesis routing keys and the identity backend together. With the variable unset,
the built-in network is unchanged. The built-in network's
other values (its TLD and dotNS controller) come from the copy bundled in the binary; the
published descriptor is not fetched, so the mode works offline. For a local network
described by a Trinity stack descriptor, pass the descriptor with `--stack` instead (see the
CLI README).

Set the variable to the JSON file's path. A blank value is the same as unset. Every field is required; misspelled
fields, a different network name, non-loopback endpoints, and malformed or duplicate
genesis hashes are rejected, and an invalid file is an error, never a silent fall back
to a live chain. Read each genesis from that local node's `chain_getBlockHash(0)`; do
not copy hashes from another run.

```json
{
  "network": "paseo-next-v2",
  "identity_backend_base": "http://127.0.0.1:8092/api/v1",
  "people": {
    "ws": "ws://127.0.0.1:10010",
    "genesis": "0x<actual People genesis>"
  },
  "asset_hub": {
    "ws": "ws://127.0.0.1:10020",
    "genesis": "0x<actual Asset Hub genesis>"
  },
  "bulletin": {
    "ws": "ws://127.0.0.1:10030",
    "genesis": "0x<actual Bulletin genesis>"
  }
}
```

`truapi-host local-network-check --network <network>` validates the file, asks each local
node for its genesis, and prints the resolved routing as JSON, so a harness can check it
before starting a session; it fails when the variable is unset. `truapi-host network-check`
prints the same routing without contacting any chain. Before any other command runs, each
local endpoint is asked for its genesis and must answer with the file's; one that does not
answer, or reports another genesis, stops the command.

Use a separate host state directory and a disposable identity for the local run:

```sh
HOST_CLI_LOCAL_NETWORK_CONFIG=/absolute/path/local-network.json \
  truapi-host dev \
    --network paseo-next-v2 \
    --base-path /absolute/path/local-host-state \
    --product-id myapp.paseo \
    --app-port 5183 -- npm run dev
```

For Previewnet select `previewnet` in both places and use `myapp.testnet`. Ports are
illustrative: use the endpoints of the local nodes you started. The complete local
configuration takes precedence over `HOST_CLI_IDENTITY_BACKEND_BASE`.

The variable cannot be combined with `--chain-backend light-client`: the embedded light
client finds each chain by genesis hash in its bundled catalog, which holds no local
network. With both set, every command that resolves a network (and `network-check`)
exits with an error naming both instead of starting.

It cannot be combined with `--stack`, which routes every chain itself, or with
`HOST_CLI_PEOPLE_WS`, `HOST_CLI_ASSET_HUB_WS` or `HOST_CLI_BULLETIN_WS`. Those point a
network at a fork with the same genesis hashes; this file fits a network with its own
genesis hashes. With both set, the commands exit with an error naming both.

The routing applies to every chain role the host serves, including the Bulletin node
the CLI asks for preimages (`bitswap_v1_get`), so the local Bulletin node must expose
that RPC for preimage lookups to succeed.

The file is syntax- and routing-validated, not an on-chain attestation. Before a real
session, check the actual genesis hashes, the product suffix, that finalized blocks
advance, and that the identity backend is ready.
