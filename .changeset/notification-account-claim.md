---
"@parity/truapi-host": minor
---

A web signing host can register an account it derives as today's notification statement account.
`claimNotificationAccount(target, submit)` on the web worker runtime, over `claim_notification_account` on the Rust
signing host runtime, proves the session's personhood ring membership and submits
`Resources.set_notification_statement_account_for_sequence` in the first free slot of the day, or with `submit` false
only asks the runtime whether it would accept the claim and submits nothing. An account that already holds today's
registration claims nothing again. Claims run one at a time; a submission the pool already holds, or whose answer is
lost, is settled by what the chain holds and never by claiming a second slot meanwhile. The claim stops using the
session's personhood keys once the session ends, so nothing is submitted after a logout. The host holds the account's
key and signs its one statement; no secret crosses the call, and products have no call that reaches it. Signing hosts on
the `testing` bundle only; the iOS and Android host runtimes do not expose it.
