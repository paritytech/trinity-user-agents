# Local-network routing for the CLI host

`HOST_CLI_LOCAL_NETWORK_CONFIG` points the CLI host at a disposable local copy of a
supported test network. It keeps `--network paseo-next-v2` / `--network previewnet`
and their product namespaces, and replaces all chain endpoints, genesis routing keys
and the identity backend together. It does not change the shipped live presets: with
the variable unset, behaviour is unchanged.

Set the variable to an absolute JSON file path. Every field is required; misspelled
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

`truapi-host local-network-check --network <preset>` validates the file and prints the
resolved routing as JSON without contacting any chain, so a harness can check it before
starting a session.

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

The variable cannot be combined with `TRUAPI_LIGHT_CLIENT=1`: the embedded light client
finds each chain by genesis hash in its bundled catalog, which holds no local network.
With both set, every command that resolves a network (and `local-network-check`) exits
with an error naming both variables instead of starting.

The routing applies to every chain role the host serves, including the Bulletin node
the CLI asks for preimages (`bitswap_v1_get`), so the local Bulletin node must expose
that RPC for preimage lookups to succeed.

The file is syntax- and routing-validated, not an on-chain attestation. Before a real
session, check the actual genesis hashes, the product suffix, that finalized blocks
advance, and that the identity backend is ready.
