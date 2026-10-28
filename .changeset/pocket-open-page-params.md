---
"@parity/truapi-host": minor
---

`NavigateDecision::Pocket` carries an `open` link's query parameters other than `product` and `card` in `page_query`, still encoded, and keeps them in `canonical_url`. Over 2048 bytes of them is rejected. Swift code matching `.pocket` by position needs a slot for the new field.
