---
"@parity/truapi-host": patch
---

A render stream the host opened fails with a host failure when its product connection closes, instead of finishing. A finished stream is a body the product completed, so the host keeps it as final; a failed one is a body the host can request again on the next connection. A product that ends a body itself still finishes the stream.
