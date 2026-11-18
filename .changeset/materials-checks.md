---
"@parity/truapi-host": patch
---

Native hosts keep only materials that pass format version 1, read as Shaderlab's reference parser reads it. A material of a format version or feature the core does not read is reported as unsupported, and a broken one as invalid. Hosts change nothing.
