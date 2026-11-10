---
"@parity/truapi": minor
---

The core holds the funding rules and state both native apps share, as facts each host presents its own way:

- `FundingOptions` carries the routed `rails`, the resolved `asset`, the crypto `tokens` and the `detected_country`;
  `FundingSelection` takes the user's `currency`. `get_funding_quote(intent, amount)` builds the ask from that selection and keeps a running request for the same ask;
  it asks only providers routing the rail, `funding_quotes` answers only the current request's list, and
  `FundingQuoteList` carries the `amount` it prices, the shared `refusal`, the amount `issue` and whether it is
  `complete`. `start_funding_when_quoted` starts on the best quote once every provider answered; `funding_available`
  says whether any provider is offered.
- `FundingProgress.cancel` says whether a cancel is possible again; `FundingSessionSummary.ended` holds how and when
  a session ended, the steps its direction goes through and where a pending payout waits; `funding_network_name`
  gives a network's English name. `funding_amount_issue` and `funding_status_in_flight` share the rules. A cancel that moved nothing is
  acknowledged as it ends and left out of `funding_history`.
- `NativeFundingCallbacks::funding_changed(intent, kind)` tells the host what changed; it reads back
  `funding_session`, `funding_progress`, `funding_options`, `funding_quotes`, `funding_entries` and `funding_summary`.
  `funding_session_open` says whether a session can be shown again. The core closes a provider frame once the payment
  moved, and `present_provider_frame` takes a `FundingFrameContext`.
- `set_funding_provider_config` follows remote configuration refreshes. `product_source` decides once whether a
  product runs from the app's bundle, a development server or dotNS. `bundled_products` (with each product's root
  manifest name and icon) and `bundled_funding_providers` read a host's bundle through a `BundleReader`, and
  `archive_file` picks the file and MIME type that answer a request into a bundled App or a published archive. The
  `funding_page_*` functions hold the provider page rules, including any https main-frame navigation with the header
  showing the current page's origin; `funding_resume_pending` says at launch whether funding should resume.
