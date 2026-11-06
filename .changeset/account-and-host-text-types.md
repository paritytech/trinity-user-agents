---
"@parity/truapi": patch
---

An account named in a log line is written through `logging::Account`, which prints a short handle under both `Display` and `Debug` and gives the whole value only through `in_full`. A value that holds an account inside it, such as a statement renewal target or a signer, redacts that field in its own `Debug`, so naming it with `?` keeps the account out of a crash report. Text a host hands the core is written through `logging::HostText`, which quotes and escapes it the way a Rust string literal is written and stops at a bound, so a foreign exception cannot break the line, be read as the fields beside it, or reorder what follows it.
