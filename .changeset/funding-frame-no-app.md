---
"@parity/truapi": minor
---

`presentFrame` refuses an App route with `NoApp` when the provider ships no App, which the core asks the host through
`provider_has_app`. A provider's App is optional: one without it shows external pages.
