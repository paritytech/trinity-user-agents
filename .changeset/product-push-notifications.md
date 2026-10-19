---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

`push.address` gives a product an opaque push address for one of its rooms, and `push.notify` asks the host to show a title and a text on the device of an address that a contact gave. The core encrypts the notification for that device, and the host sends it through its push relay. A product sends at most 30 notifications in 60 seconds. The iOS app opens product pushes in its notification service extension.
