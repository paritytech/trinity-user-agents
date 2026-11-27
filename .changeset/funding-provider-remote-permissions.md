---
"@parity/truapi-host": minor
---

A funding provider the host lists in its funding configuration gets every `RemotePermission` without a prompt, as a
privileged product does, for as long as it is listed. `hasTrustedRemotePermissions` answers `true` for it. Consent
between products, the `Context` manifest scope and every other first-party rule stay with the privileged products.
