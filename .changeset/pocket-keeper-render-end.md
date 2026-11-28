---
"@parity/truapi-host": patch
---

Native hosts keep running when a Pocket card render ends right after drawing a face. The task that keeps the card's newest face writes that face and ends with the render.
