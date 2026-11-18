---
"@parity/truapi": minor
---

The core hands a host a structured fact as well as a log line. `CoreEvent` builds one from a marker and typed attributes and `HostCallbacks::on_core_event` carries it over, so a host gives its reporter attributes it can group and filter by instead of prose it can only store. A key is written in the core's own source and a value is a label, a count or a condition, each bounded on the way out. One marker table gates both ways out of the core, and `HostBridge` defaults the new callback to nothing on iOS and Android.
