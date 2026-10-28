---
"@parity/truapi-host": minor
---

A `polkadotapp://pocket/open` link keeps its query parameters other than `product` and `card`. `NavigateDecision::Pocket` carries them in `page_query`, as the link wrote them and in its order, for the card's page to load with after `card`, and keeps them in `canonical_url`, so a product's `navigate_to` passes them on too. An `add` link forwards none. A link carrying over 2048 bytes of them is rejected. Swift code that matches `.pocket` by position needs a slot for the new field.
