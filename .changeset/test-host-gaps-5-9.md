---
"@parity/truapi-host": minor
---

The test host gains what migrating suites were missing:

- `injectStatement` takes `channel`, `expiry` and `decryptionKey`, and every statement entry reports them.
- `setConfirmUserActions` switches signing confirmations at runtime, until `reset`. A product holding an auto-signing
  allowance is not asked, as on any host.
- With `loginBehavior: "manual"`, a product's login request waits for `approveLogin(name)` or `declineLogin()` instead
  of being rejected straight away. An answer given before the request answers the next one. `reset` declines a held
  request and signs the host out again. Both answers throw under `loginBehavior: "auto"`.
- The host page passes its path, query and hash on to the product as a deep link, under `productUrl`'s own path and
  query, and `testHost.goto(deepLink)` reopens the page at one. The host page's own parameters never reach the product,
  and a link to another origin, to one of the server's own files, or with a query name the host page reads itself is refused.
- `injectStatement` refuses a `channel` or `decryptionKey` that is not 32 bytes, and an `expiry` outside a u64.
