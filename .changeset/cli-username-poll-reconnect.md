---
"@parity/truapi": patch
---

The CLI host's dotNS reads on Asset Hub redial on their own after a dropped or silent socket, so one connection reset during provisioning no longer fails every remaining username poll.
