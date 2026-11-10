---
"@parity/truapi": major
---

The core holds the funding rules and state both native apps share, as facts each host presents its own way.

Breaking for native hosts:

- `NativeFundingCallbacks` loses `funding_session_changed`, `funding_in_progress_changed`, `funding_options_changed`,
  `funding_quotes_changed` and `provider_has_app`, and gains `funding_changed(intent, kind)`, after which the host reads
  `funding_session`, `funding_progress`, `funding_options`, `funding_quotes`, `funding_entries`, `funding_summary` and
  `funding_session_open` back.
- `get_funding_quote(intent, amount)` replaces `get_funding_quote(intent, ask)`: the core builds the ask from the last
  `select_funding_options` and returns `None` while there is nothing to price.
- `NativePaymentCallbacks::payment_status`, `NativeTopUpCallbacks::top_up_status` and `NativeBalanceCallbacks::balance`
  become `follow_payment`, `follow_top_up` and `follow_balance`; `notify_balance` takes no purse.
- `FundingProviderEntry` gains `root_manifest`, and `has_app` becomes `app: FundingProviderApp`.

The rest:

- `FundingOptions` carries the routed `rails`, the resolved `asset`, the crypto `tokens` and the `detected_country`, and
  each `FundingCandidate` its `brand`. `FundingQuoteList` carries the `amount` it prices, the shared `refusal`, the
  amount `issue` and whether it is `complete`; `funding_quotes` answers only the current request's list.
- `FundingProgress` carries `cancel` and `awaits_user_transfer`; `FundingSessionSummary.ended` holds how a session
  ended, its steps and where a pending payout waits. `funding_amount_issue`, `funding_status_in_flight` and
  `funding_network_name` share the rules. A cancel that moved no money is left out of `funding_history`.
- `start_funding_when_quoted` starts on the best quote once every provider answered; `funding_available` says whether
  any provider is offered.
- The core follows payment, top-up and balance streams: pending after 250 ms, the last 512 final statuses kept, an
  engine that stops early ending the stream with the abandoned status.
- An App route is checked against the provider's `app` subname on dotNS. A Worker manifest needs an `entrypoint`, an
  `appVersion` and a surface; a brand needs a name, a description and an icon.
- `product_source`, `bundled_products`, `bundled_funding_providers` and `archive_file` decide where a product is served
  from and which file answers a request; the `funding_page_*` functions hold the provider page rules, and
  `funding_resume_pending` says at launch whether funding should resume.
