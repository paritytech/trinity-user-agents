---
"@parity/truapi-host": patch
---

dotNS contract reads encode calls and decode return data through typed Solidity interfaces, and reject return data that is not canonically encoded. A revert reason reads as `revert: <reason>` or `panic: <kind> (<code>)`.
