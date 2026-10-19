---
"@parity/truapi-host": major
---

`parse_navigate` reads a Pocket deeplink as the host's own URL. `polkadot://pocket/add?product=<id>&card=<id>` and `polkadot://pocket/open?product=<id>&card=<id>` classify as `NavigateDecision::Pocket`, naming the product from the `product` parameter, and `polkadot://pocket` classifies as `NavigateDecision::PocketCollection`, which carries no `identifier`. A Pocket link with an action the core does not serve, or with no product or no card, is rejected. A product's own paths are App routes: `polkadot://<product>.<tld>/-/pocket/...` classifies as `DotName` and opens the product, so a product that links to the Pocket uses the new form. `navigate_to` passes a Pocket link without an `OpenUrl` grant.
