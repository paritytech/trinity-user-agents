---
"@parity/truapi": minor
---

With `TRUAPI_DECISIONS_DIR` set and no `--auto-accept`, the CLI host publishes each confirmation to that directory and waits for another process to answer it, so a script can approve or deny requests one at a time. `TRUAPI_DECISIONS_TIMEOUT_MS` bounds each wait (default 10 minutes); an unanswered request is denied. The directory is refused on a preset that is not a test network.
