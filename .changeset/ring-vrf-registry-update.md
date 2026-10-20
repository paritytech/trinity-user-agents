---
"@parity/truapi-host": patch
---

Apply every ring-VRF registry change through one prepared update under the registry's storage lock, and skip the storage write when the registry did not change. Test-host allocation controls live in one `TestResourceControls`.
