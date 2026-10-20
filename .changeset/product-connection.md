---
"@parity/truapi-host": patch
---

Keep each product connection's adapters, permissions and action channels in a `ProductConnection`, reach account operations through a `ProductAccounts` bound to that connection's product, and route login, logout and connection state through a `HostSession` trait split out of `ProductAuthority`.
