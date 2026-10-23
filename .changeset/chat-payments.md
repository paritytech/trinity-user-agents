---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add payments to the contact rooms of Chat Extension v2. The user action `SendPayment` carries a payment the host planned, reserved and handed off with its own coinage flow: the message id and time the host chose, the total value and the 64-byte key of each coin (at most 1,024). The product sends the payment message to the contact under that id and answers `MessageCreated` with a `Payment` body; the keys appear in no body, preview or reply quote. `personChat.paymentReceived` hands the host the coin keys of a payment a contact sent, and the host claims the coins under the message id; a later call for the same message changes nothing. `PersonChatPlatform` gains `payment_received`, which answers `HostChatPaymentReceipt::Unsupported` unless a host implements it; the CLI host roles keep that answer. Native hosts implement `NativePersonChatCallbacks.payment_received`; `PersonChatHostBridge.paymentReceived` on iOS and Android defaults to unsupported. The protocol carries no claim state: hosts draw their own over the `Payment` message by its id.
