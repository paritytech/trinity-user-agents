---
"@parity/truapi": minor
---

A funding provider closes the screen it showed for a session with `fundingProvider.closeFrame({ intent })` once it is
done with it, such as a card payment the user made. The core asks the host through the new `close_provider_frame`
callback; the open `presentFrame` answers `Closed` and the host shows the session sheet in its place. A session the
provider does not serve answers `NotFound`.
