---
"@parity/truapi-host": minor
---

A `polkadotapp://pocket/<action>` link keeps its query parameters other than `product` and `card`. `NavigateDecision::Pocket` carries them in `page_query`, percent-encoded and in link order, for the card's page to load with after `card`, and keeps them in `canonical_url`, so a product's `navigate_to` passes them on too. A link carrying over 2048 bytes of them is rejected.
