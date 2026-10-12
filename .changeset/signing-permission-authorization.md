---
"@parity/truapi-host": patch
---

A Web Worker host booted with `role: 'signing'` serves the same host surface as a pairing host:

- `getPermissionAuthorizationStatus`, `getPermissionAuthorizationStatuses` and `setPermissionAuthorizationStatus` answer, so a signing host can read stored device permissions when it mounts a product. `WasmSigningHostRuntime` exposes `permissionAuthorizationStatus`, `permissionAuthorizationStatuses`, `setPermissionAuthorizationStatus` and `notifyContactsChanged` with the same signatures as `WasmPairingHostRuntime`.
- The optional `chat`, `contacts` and `permissionStatus` callback groups reach the core. A device capability the browser blocks reads as `Denied` whatever is stored, and on the test host a Worker product's chat calls reach the mock, so the Playwright fixture's `getChatRooms`, `getChatBots` and `getChatMessageLog` record them.
- A request only a pairing host can answer, such as `getSessionChatIdentityKey` or `activateStoredSession`, rejects with an error naming the request. `cancelPairing` and `notifySessionStoreChanged` do nothing, since a signing host has no pairing and no stored session.
