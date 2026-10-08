---
"@parity/truapi": minor
"@parity/truapi-host": major
---

Add `chat.setRoomFooter`, which sets what a room the product created shows below its messages: the text input, or nothing for a room the product drives through actions alone. A host keeps the footer until the product sets another, and a host that predates the method answers `Unsupported`. A host serving chat now implements `setChatRoomFooter`, and native hosts implement `ChatHostBridge.setRoomFooter`.
