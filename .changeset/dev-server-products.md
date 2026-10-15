---
"@parity/truapi-host": patch
---

Native hosts recognise a development server through the core. `parse_dev_server` reads an address a developer typed, such as `localhost:3000`, `10.0.2.2:3000` or `192.168.1.59:3000?product_id=humanity.dot`, and returns a `DevServerProduct`: the identifier the product runs under, the `http` origin it is served from, and the address to load. The identifier is the normalized `product_id` the address names, otherwise `localhost[:port]`. Only `localhost` and loopback or private IPv4 hosts over `http` qualify, and an address that does not raises `DevServerError`. Rust callers parse the address with `str::parse::<DevServerProduct>()`. A `localhost` identifier holds the core's development wildcard and a named one gets that product's accounts and storage, so hosts call it only in development builds. Native builds only.
