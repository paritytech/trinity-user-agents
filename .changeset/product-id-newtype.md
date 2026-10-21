---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

`truapi-host` trims and lowercases the `productName` of a local product config before storing its grants, so
`Peopl.Paseo` grants the same as `peopl.paseo`. The name must be a full product id: a bare label such as `peopl`, or a
name such as `example.com`, stops the CLI at startup with an error, before any config is stored.

Reading the cross-product signature permission for a target that is not a valid product id returns "not determined",
and setting it returns an error.
