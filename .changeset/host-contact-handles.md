---
"@parity/truapi-host": minor
---

A web host can mint the contact handles `contacts.pick` gives its products, so it can draw a handle a product holds as
the person it names. `contactHandles(accounts)` on the web worker runtime, over `contact_handles` on both Rust host
runtimes, answers one 32-byte handle per account, in order, under the current session, or nothing without one. The iOS
and Android host runtimes do not expose it yet. It writes no cache: a minted handle resolves in a transaction only while
the host's contacts lookup still answers for its account. Products get no new call.
