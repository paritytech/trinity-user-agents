---
"@parity/truapi": patch
---

Signing hosts convert funding deposits into CASH on People once `enable_funding_conversion` gives the network's CASH asset id (Rust API). A CASH deposit is teleported; a stablecoin the PSM serves is minted into CASH first. Fees are paid in the deposited asset, and both chains dry-run the conversion before it is submitted.
