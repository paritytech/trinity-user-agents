---
"@parity/truapi-host": major
---

The core notices that a stack's chains restarted, as after a wipe, by the descriptor's `generation` and the genesis
hashes together, so a wipe is seen even when the operator left `generation` as it was. What it stored about the old
chains is not used again: a cached product, Worker or App manifest is read again from the new Asset Hub, stored
allowance keys and ring-VRF registries read as empty, and the usernames of a stored session are looked up again before
it is installed. The session itself and stored permissions are kept. Manifests, allowance keys and registries are
stored with the run they were read in, and the run of the stored session's usernames is recorded in the new
`SessionUsernamesRun` slot. Manifests, allowance keys and registries stored before this change carry no run and are
dropped once, so a product asks for its allowance again, and a stored session's usernames are looked up once. The
usernames in a blob passed to `activateExternalSession` are looked up again on every activation, unless it is the blob
in the `AuthSession` slot.
