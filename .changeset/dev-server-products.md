---
"@parity/truapi-host": patch
---

Native hosts recognise a development server through the core. `parse_dev_server` reads an address a developer typed, such as `localhost:3000`, `10.0.2.2:3000` or `192.168.1.59:3000`, and returns a `DevServerProduct` with the `localhost[:port]` identifier the product runs under and the `http` origin its page loads from. Only `localhost` and loopback or private IPv4 hosts over `http` qualify. The identifier holds the core's development wildcard, so hosts call it only in development builds.
