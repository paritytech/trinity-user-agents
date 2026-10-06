---
"@parity/truapi-host": minor
---

Native StatementStore and Bulletin allowance keys persist through the typed `NativeAllowanceKeys` secret slot, bound to the wallet and product. Wallet lock preserves them; scoped reset and rejection cleanup remove only the intended grants. Existing mobile allowance stores are not migrated.
