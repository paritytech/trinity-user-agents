---
"@parity/truapi": minor
---

A host reports a product worker that stopped without being asked, because it crashed, failed to boot or the system
killed it, with `worker_exited` (`workerExited` on the web). While anything still holds that worker, the core requests
`Start` again through `request_worker_transition`, and hands a funding provider's restarted worker every session it
serves once it connects. Crashes less than a minute apart back off: the first restart is immediate, then each waits 1 s,
doubling up to 5 min. On the third crash in a row the host hears the new `worker_crashing(product_id, crashes)`
(`workerCrashing` on the web) and tells the user that product is not working. A released withdrawal's provider can
report `Details` for its payout until it reports the payout.
