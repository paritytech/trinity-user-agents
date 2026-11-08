---
"@parity/truapi-host": minor
---

A web signing host can keep a backup of its own on the network through two accounts the core derives from the wallet
root under the reserved `host-backup` junction, outside every product subtree and product allowance account. On the web
worker runtime, `backupAccount()` answers the backup statement account's public key;
`claimBackupStatementAccount(submit)` gives it a general Statement Store slot for the current UTC day, reusing one it
already holds, or with `submit` false only validates the claim; `signBackupPointer(payload)` signs a backup pointer
statement's unsigned signing payload and refuses any payload that is not a canonical unsigned statement with a channel
and data starting with `twhb/ptr/1`; and `storeBackupBlob(value)` stores an encrypted snapshot on Bulletin under the
backup Bulletin account, claiming long-term storage first when its authorization cannot hold the value, and answers the
preimage key. No secret crosses these calls, and products have no call that reaches them. Signing hosts on the `testing`
bundle only; the iOS and Android host runtimes do not expose them.
