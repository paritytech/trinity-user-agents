---
"@parity/truapi-host": major
---

The core notices that a stack's chains restarted, as after a wipe, by the descriptor's `generation` and the Asset Hub,
People and Bulletin genesis hashes together, so a wipe is seen even when the operator left `generation` as it was. What
it read from the old chains is not used again: a cached product, Worker or App manifest is read again from the new Asset
Hub, stored allowance keys are ignored and replaced on the next write, the ring-VRF registry loses the rings on chains
the stack no longer runs, and a session's usernames are looked up again. The session itself and stored permissions are
kept. Manifests and allowance keys are stored with the run they were read in. The new `SessionUsernamesRun` slot holds
the run in which the usernames of the `AuthSession` blob were read, with a hash of that blob; a host that keeps
`AuthSession` under a key of its own keeps `SessionUsernamesRun` next to it.

A session is installed without waiting for its usernames, which are looked up afterwards; a lookup that fails changes
nothing. A blob passed to `activateExternalSession` that the core does not vouch for has its usernames looked up again
on every activation. On an update with no wipe, a pairing host asks for each product's allowance again, because the
stored keys carry no run, and a stored session's usernames are looked up once in the background. A manifest cached
before this change is read again once.
