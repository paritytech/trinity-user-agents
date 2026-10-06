---
"@parity/truapi": patch
---

Funding deposits handle getcash's edge cases: every deposit asset is read on each deposit account, so a short or wrong-asset deposit shows as a mismatch; `accept_funding_deposit` converts what arrived instead, and reopens a session that expired or had its conversion refused for 72 hours after; a conversion that lands less than expected credits what landed; and `funding_account_secret` exports an account's seed for a wallet. `enable_funding_conversion` takes the deposit asset ids (Rust and native API).
