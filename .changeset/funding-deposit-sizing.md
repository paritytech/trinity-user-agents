---
"@parity/truapi": patch
---

Signing hosts quote the deposit a funding session needs with `quote_funding_deposit`: enough to credit the session's amount after the fee on People, the PSM fee, transaction fees and the minimum balance (Rust API). Assigning a deposit account refuses a smaller expected deposit, and an unset PSM minting fee reads as the pallet's default. `funding-check` takes the CASH amount to credit.
