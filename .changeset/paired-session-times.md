---
"@parity/truapi-host": major
---

A pairing host ends a paired session 30 days after pairing. The core records the time a session was paired in the new
`SessionTimes` core storage slot, and when it restores the session in the `AuthSession` slot, at boot or after a store
change, it clears a session paired 30 days ago or more, with its automatic-signing and allowance grants. A session
stored without times keeps working, and its 30 days start at the first restore that sees it. Hosts back `SessionTimes`
with the same store as `AuthSession`, shared the same way: a host that routes core storage slots by key, as dot.li does,
routes `SessionTimes` with `AuthSession`. The core deletes `PairingDeviceIdentity` once a login commits or a stored
session is restored, since the session holds the same keys.
