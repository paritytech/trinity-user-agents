---
"@parity/truapi": patch
---

Give each product runtime its own chain connection. A node budgets follows and concurrent operations per connection, so one connection shared across products made those budgets collide. A product runtime's connections are released when it ends.
