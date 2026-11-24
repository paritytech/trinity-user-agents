---
"@parity/truapi-host": major
---

After a wipe a stack keeps its id but its chains are new, and the core kept using what it had read from the old ones. It
now stops: cached product, Worker and App manifests are read again, allowance keys are asked for again, the ring-VRF
registry loses the rings on chains that are gone, and the session's usernames are looked up again. The session and
stored permissions stay.

The core tells a restart by the descriptor's `generation` and the Asset Hub, People and Bulletin genesis hashes, so a
wipe that changes a hash is caught even when `generation` was left as it was.

A restored session comes up without waiting for its usernames. The core looks them up afterwards and records in the new
`SessionUsernamesRun` slot which run it read them in. A host that keeps `AuthSession` under a key of its own has to keep
`SessionUsernamesRun` next to it.

On the first start after the update, with no wipe, cached manifests are read again, a pairing host asks for each
product's allowance again, and the stored ring-VRF registry is not read, so a provider chosen for a ring has to be
chosen again.
