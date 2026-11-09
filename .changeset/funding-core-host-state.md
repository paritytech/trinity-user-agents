---
"@parity/truapi": minor
---

The core keeps the funding state a native host reads, and the shared host plumbing around it:

- `NativeFundingCallbacks` tells the host what changed with `funding_changed(intent, kind)`; the host reads the
  rest back: `funding_session`, `funding_progress`, `funding_options`, `funding_quotes`, `funding_entries`,
  `funding_unacknowledged`, `funding_in_progress` and `funding_payment_moved`.
- `funding_activity` lists sessions in flight and the history of ended ones as rows of facts, hiding a cancel before
  paying that settled and refunded nothing; `acknowledge_funding_shown` acknowledges what the user saw.
- The core closes a provider's frame once the session's payment moved. `present_provider_frame` takes a
  `FundingFrameContext` with the App route's path and whether the user transfers outside the frame.
- `set_funding_provider_config` merges the remote configuration's provider names with the bundled providers and
  ignores a refresh that changes nothing. `bundled_funding_providers`, `bundled_products`, `bundled_asset_path` and
  `mime_for` read a host's bundled products through its `BundleReader`; a bundled entry's `has_app` answers whether
  the provider ships an App.
- `funding_page_matches_return`, `funding_page_allows_navigation`, `funding_page_origin` and
  `funding_page_allows_media` hold the provider page rules, `funding_resume_pending` says at launch whether funding
  should resume, and `WorkerLaunchTable` tells a worker's live launch from one it replaced.
