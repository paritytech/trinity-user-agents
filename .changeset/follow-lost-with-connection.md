---
"@parity/truapi": patch
---

A chain-head call on a follow whose connection closed under it now says so, instead of reporting the id as unknown. A follow does not survive its transport: the core opens a replacement connection and does not re-follow, so the id the caller still holds stops resolving and every call bound to it fails until a new follow is opened. Both outcomes used to read `unknown follow subscription id`, which is also what a genuinely unrecognised id returns, so a caller could not tell a dead follow from a bad one and neither could anyone reading a crash report. A scope now remembers the follows it lost that way, bounded, and answers `follow subscription "<id>" ended when its connection closed` as `Unavailable` rather than a `HostFailure` blaming the caller.
