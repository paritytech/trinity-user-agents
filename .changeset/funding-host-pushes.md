---
"@parity/truapi": major
---

A native host reads back only `funding_history()`, whenever it shows history, which lists every ended session, with its
provider, rail, asset, status, payout, refund and references, as soon as it ends; the core pushes everything else it
draws. `funding_session_changed` carries the session as the core holds it (choice, cancel offer, amounts) beside its
status and progress. `funding_options_changed` carries the overlay's candidates, and the countries and amount limits for
the rail, asset, network and country the host reports with the new `select_funding_options`, with the limits on each
declared crypto network; it arrives before the overlay shows, after each selection and after each quote list.
`funding_in_progress_changed` arrives each time funding resumes and again whenever it changes. `funding_session`,
`funding_sessions`, `funding_in_progress()`, `funding_candidates`, `funding_countries`, `funding_amount_limits` and
`detected_funding_country` are removed from the runtime. Every funding provider's worker starts as the overlay opens, so
the first quotes do not wait on a cold start.
