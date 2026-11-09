---
"@parity/truapi": minor
---

The core holds the funding rules and state both native apps share, as facts each host presents its own way:

- `FundingOptions` carries the routed `rails`, the `default_rail`, the resolved `asset`, the crypto `tokens` and the
  `detected_country`; `FundingSelection` takes the user's `currency`. A quote request asks only providers routing the
  rail, and `FundingQuoteList` carries the `amount` it prices, the shared `refusal` and the amount `issue`.
- `FundingProgress.cancel` says whether a cancel is possible again; `FundingSessionSummary.ended` holds how a session
  ended. `funding_amount_issue`, `funding_fee_breakdown`, `funding_units`, `funding_asset_is_fiat`,
  `funding_network_name`, `funding_status_in_flight` and `funding_status_delayed` share the arithmetic.
- `NativeFundingCallbacks::funding_changed(intent, kind)` tells the host what changed; it reads back
  `funding_session`, `funding_progress`, `funding_options`, `funding_quotes`, `funding_entries` and `funding_activity`,
  and calls `acknowledge_funding_shown` once history is on screen. `funding_session_open` says whether a session can be
  shown again. The core closes a provider frame once the payment
  moved, and `present_provider_frame` takes a `FundingFrameContext`.
- `set_funding_provider_config` follows remote configuration refreshes. `product_source` decides once whether a
  product runs from the app's bundle, a development server or dotNS. `bundled_products` (with each product's root
  manifest name and icon) and `bundled_funding_providers` read a host's bundle through a `BundleReader`, and
  `archive_file` picks the file and MIME type that answer a request into a bundled App or a published archive. The
  `funding_page_*` functions hold the provider page rules, including any https main-frame navigation with the header
  showing the current page's origin; `funding_resume_pending` says at launch whether funding should resume.
