---
"@parity/truapi-host": minor
---

`testHost.setLoginBehavior("success" | "reject", account?)` answers a product's login request while the test host is signed out, as a user would on a real host. `"success"` signs in the first account, or the one named, and the product gets `Success`. `"reject"` answers `Rejected`. The test host page answers the request itself, so the core is unchanged, and a request made while signed in still reaches the core, which answers `AlreadyConnected`.
