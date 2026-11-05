---
"@parity/truapi-host": minor
---

The test host passes a deep link on to the product: the host page's path, its hash and every query parameter it does not read as its own configuration reach the product's URL, as on a real host. `testHost.openDeepLink(link)` opens the product at a link from the Playwright fixture, so one fixture serves every link, and `deepLinkUrl(hostUrl, link)` from `@parity/truapi-host/testing/server` builds the same URL for a suite that starts the server itself.
