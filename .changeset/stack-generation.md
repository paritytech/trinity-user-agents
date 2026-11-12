---
"@parity/truapi-host": major
---

The core notices that a stack's chains restarted, as after a wipe, by the descriptor's `generation` and the genesis
hashes together, so a wipe is seen even when the operator left `generation` as it was. What it stored about the old
chains is dropped: a cached product, Worker or App manifest is read again from the new Asset Hub, and before a session
is used the usernames in it are looked up again and its allowance keys and ring-VRF registry are cleared. The session
itself and stored permissions are kept. The run the stored data belongs to is recorded in the new `ChainRun` slot. A
manifest cached before this change is read again once.
