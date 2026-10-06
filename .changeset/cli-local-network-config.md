---
"@parity/truapi": minor
---

The CLI host can run against a local network. `HOST_CLI_LOCAL_NETWORK_CONFIG` names a JSON file that routes every chain role (People, Asset Hub, Bulletin) and the identity backend of the selected preset to loopback endpoints, keeping the preset's product namespace. An invalid file is an error, never a fall back to the live chain. `truapi-host local-network-check` prints the resolved routing without contacting a chain. It cannot be combined with `TRUAPI_LIGHT_CLIENT=1`.
