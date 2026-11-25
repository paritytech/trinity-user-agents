---
"@parity/truapi-host": major
---

After a wipe a stack keeps its id but its chains are new, and the core kept using what it had read from the old ones. It
now stops: cached product, Worker and App manifests are read again, allowance keys are asked for again, the ring-VRF
registry loses the rings on chains that are gone, and the session's usernames are looked up again. The session and
stored permissions stay.

The core tells a restart by the descriptor's `generation` and the Asset Hub, People and Bulletin genesis hashes, so a
wipe that changes a hash is caught even when `generation` was left as it was. It orders runs by the descriptor's
`sequence`: a runtime that starts on an earlier run of the stack than the values in storage, as when the stored
descriptor could not be read, reads none of them and neither clears nor writes over them, keeping its own allowance keys
and registry updates in memory. Ending the session still clears its allowance keys in every run.

A restored session comes up without waiting for its usernames. The core looks them up afterwards and records in the new
`SessionUsernamesRun` slot which run it read them in. A host that keeps `AuthSession` under a key of its own has to keep
`SessionUsernamesRun` next to it.

On the first start after the update, with no wipe, nothing the core stored about the chains carries a run yet:

- Cached manifests are read again.
- A pairing host asks for each product's allowance again.
- The session's usernames are kept and looked up again.
- The stored ring-VRF registry is not read, and the next update replaces it, so every registration and provider choice
  in it is lost. On a signing host `ring_vrf_sign` fails with `KeyNotRegistered` until each product registers its keys
  again.
