---
"@parity/trinity-stack": minor
"@parity/truapi": minor
---

A descriptor can state its stack's dotNS contract addresses in `dotns`, by protocol registry key, so readers take them
from the descriptor instead of reading them from Asset Hub. `truapi-host stack` and `@parity/trinity-stack` validate the
keys and addresses.
