---
"@parity/truapi-host": major
---

Hosts must implement the required `secretCoreStorage` callbacks for authentication, pairing identity, device encryption identity and retained account keys. These typed slots are separate from public `coreStorage`; storage failures must propagate, and cleanup must wait for earlier storage effects to finish.

Native and CLI secrets use new storage namespaces without migration, including the CLI device identity. Native messaging device identity remains shared with the existing provider.
