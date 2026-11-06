---
"@parity/truapi": patch
---

An account named in a log line is written through `logging::Account`, which prints a short handle under both `Display` and `Debug` and gives the whole value only through `in_full`. Text a host hands the core is written through `logging::HostText`, which keeps the message on one line and stops at a bound, so a foreign exception that runs long or carries newlines cannot break the detail a host forwards or crowd out the fields beside it.
