---
"@parity/truapi-host": minor
---

`testHost.setLoginBehavior("success" | "reject", account?)` answers a product's login request while the test host is signed out, as a user would on a real host. `"success"` signs in the first account, or the one named, and the product gets `Success`. `"reject"` answers `Rejected`. Unset, a signed-out host still answers `Rejected`, unlike `@parity/host-api-test-sdk`, which approved by default, so a suite relying on that calls `setLoginBehavior("success")`. A withdrawn request is answered `Cancelled`, and `signOut`, `switchAccount` and `setAccounts` wait for a sign-in a request started. The test host page answers the request itself, so the core is unchanged, and a request made while signed in still reaches the core, which answers `AlreadyConnected`. The type `setLoginBehavior` takes is exported as `LoginRequestBehavior`.
