---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

Each product runtime holds its own chain connection, so a node's four-follow budget per connection is spent per product rather than shared across all of them. A product runtime's connections are torn down when it ends.
