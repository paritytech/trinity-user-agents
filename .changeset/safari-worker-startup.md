---
"@parity/truapi-host": patch
---

Give Web Worker WASM loading and runtime construction separate 30-second startup deadlines. Slow Safari startup no
longer terminates a worker that has already reached its next initialization milestone.
