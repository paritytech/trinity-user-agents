---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A chain call that fails because its connection dropped or closed carries the reason the transport gave, as in `remote_chain_spec_genesis_hash: RPC error: client error: json-rpc connection closed`. Each failed chain call is also logged as a warning with its failure kind, and a native host receives warnings and errors through `on_core_log` as `truapi.tracing.warn` and `truapi.tracing.error` lines, whatever the log level, each detail capped at 512 characters.
