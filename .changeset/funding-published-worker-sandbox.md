---
"@parity/truapi": patch
---

The `truapi-host` signing host runs funding providers' workers published on dotNS in an operating-system sandbox (`sandbox-exec` on macOS, Landlock on Linux) with a cleared environment: a worker reads only the runner and its own archive, writes only a scratch directory, executes nothing but `bun`, and connects only to its frame socket and TCP ports 80 and 443. Where it cannot be sandboxed the host refuses to run it unless `TRUAPI_FUNDING_UNSANDBOXED=1`.
