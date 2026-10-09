---
"@parity/truapi-host": minor
---

`parse_navigate` classifies `polkadot://<product>.<tld>/-/pocket`, with no action, as `NavigateDecision::PocketCollection`: a product's link to the Pocket itself rather than to one card. Like a card link it resolves into the host's own surface, so `navigate_to` passes it without an `OpenUrl` grant. A host that matches `NavigateDecision` exhaustively needs an arm for the new variant.
