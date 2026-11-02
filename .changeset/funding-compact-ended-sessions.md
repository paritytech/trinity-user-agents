---
"@parity/truapi": minor
---

An ended funding session is compacted into the session pack as soon as it ends, acknowledged or not, unless its provider
still works on it or a late payment may still open it. The pack remembers whether the host acknowledged each session:
`funding_session_changed` hands the host the full session with the change that ended it, and the 200 newest sessions it
has not acknowledged come through again as summaries when funding resumes. `acknowledge_funding_session` marks a
session told to the user whether it is held in full or compacted. Compacted sessions stored before this release are
skipped and kept.
