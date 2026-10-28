---
"@parity/truapi-host": minor
---

`NavigateDecision::Pocket` carries an `open` link's query parameters other than `product` and `card` in `page_query`, still encoded, and keeps them in `canonical_url`. A link with over 2048 bytes of them, or with characters a URL query cannot hold, is rejected. Swift code matching `.pocket` by position needs a slot for the new field.
