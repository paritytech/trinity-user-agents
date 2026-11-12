---
"@parity/trinity-stack": patch
---

A reader on a `maxAge` policy ignores a request time that is ahead of the clock, as it already ignored such a fetch
time. A clock that is corrected backwards no longer delays the next fetch by the size of the correction. The
`trinity-stack` crate's `StackReader` also has `due_in`, the seconds until its refresh policy makes a stack's fetch due,
so a caller sleeps that long instead of polling `get`.
