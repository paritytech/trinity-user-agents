---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A product id must be a name dotNS can register, or `localhost[:port]`. Each label is lowercase ASCII letters, digits and
hyphens, does not start or end with a hyphen, and is at most 63 bytes, as the dotNS registrar requires. Any case is
accepted and lowercased. An id such as `tést.dot`, `my_app.dot`, or one carrying an invisible character is refused
wherever the runtime parses a product id.
