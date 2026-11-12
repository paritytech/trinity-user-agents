---
"@parity/trinity-stack": minor
---

The `trinity-stack` crate's `StackReader` has `due_in`, the seconds until its refresh policy makes a stack's fetch due.
A caller sleeps that long instead of polling `get`. The JavaScript reader is unchanged.
