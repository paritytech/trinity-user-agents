---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`callMedia.start`, `callMedia.signal` and `callMedia.stop` let a chat product run the audio and the video of a call in one of its rooms with the call engine of the host, within 180 seconds of a press on a call button or on Accept in that room. The host gives its media signals to the product as the room event `CallSignal`. The iOS app runs the call engine and the call screen of its native chat.
