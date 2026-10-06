---
"@parity/truapi": patch
---

`set_withdrawal_payout` names the Asset Hub account a withdrawal pays out to; the withdrawal account then sends everything there with `transfer_all`, never into a channel about to close, and the session ends `Released` (Rust and native API).
