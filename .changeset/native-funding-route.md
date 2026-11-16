---
"@parity/truapi": major
---

The core runs a native funding route for top-ups on the stack's own network. It is listed first among funding candidates
while Asset Hub serves any of DOT, dotUSD, USDT or USDC through the pools and the PSM, read live. It takes a deposit on a
session key derived from the user's seed, converts it with an XCM program, and credits the CASH through the host's
`top_up` under the product id `native:funding`.

Breaking for native hosts:

- `FundingCandidate` gains `native`, true for the core's own route. The host names and brands that candidate itself.
- `set_native_funding_destination(intent, address)` records the Asset Hub account a native withdrawal pays out to.
  Withdrawals are not served yet.
