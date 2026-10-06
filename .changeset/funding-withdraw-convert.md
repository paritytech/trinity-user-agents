---
"@parity/truapi": patch
---

A paid withdrawal's CASH moves to Asset Hub as getcash moves it: a fee swap on People, then one XCM sized by dry runs on both chains that sells the CASH for PAS on Asset Hub, landing on the withdrawal account (`Withdrawn`).
