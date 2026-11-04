---
"@parity/truapi": patch
"@parity/truapi-host": patch
---

A product that navigates its whole document inside an iframe host connects again.

- `@parity/truapi`: a product addresses its iframe handshake to the embedding page's origin, taken from `location.ancestorOrigins` first. A referrer on the product's own origin, which is what a document loaded by the product's own navigation carries, no longer stands in for the host.
- `@parity/truapi-host`: `createIframeHost` answers each new document's `truapi-ready` with a port of its own and moves the connection to it once the document sends a frame. `onPort` is called again with that port, and the function it returned for the previous port runs when it is replaced or the host is disposed. A stray ready from a document that already holds its port leaves its connection alone.
- The test host gives each product document its own core, so a suite whose product sets `location.href` keeps talking to the host.
