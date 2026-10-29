---
"@parity/truapi": major
---

A native host reads nothing back from funding; the core pushes what it draws. `funding_session_changed` carries the
session as the core holds it (choice, cancel offer, amounts) beside its status and progress. `funding_options_changed`
carries the overlay's candidates, and the countries and amount limits for the rail, asset, network and country the host
reports with the new `select_funding_options`, with the limits on each declared network for a crypto asset; it arrives
before the overlay shows, after each selection and after each quote list. `funding_history` and
`funding_in_progress_changed` arrive each time funding resumes, the latter again whenever it changes. `funding_session`,
`funding_sessions`, `funding_history()`, `funding_in_progress()`, `funding_candidates`, `funding_countries`,
`funding_amount_limits` and `detected_funding_country` are removed from the runtime.
