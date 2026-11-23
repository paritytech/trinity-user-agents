---
"@parity/truapi": patch
---

`truapi-host alloc-check --submit` prints `REGISTERED` only once the allowance is readable at the People node's best block, the state the node checks a statement against, and adds `best=<hash>` to that line. A statement published after `REGISTERED` is no longer rejected with `noAllowance` while the registration's block is on a fork the node has not made best. When the allowance isn't at any best block within 60 seconds the command fails.
