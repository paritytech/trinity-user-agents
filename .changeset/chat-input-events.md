---
"@parity/truapi": minor
"@parity/truapi-host": minor
---

Add chat input events: a room that declares `input_events` gets `ChatEvent::InputChanged { text }` with the text of the input field while the user types, at most 256 bytes (`CHAT_INPUT_EVENT_MAX_BYTES`), so the product can suggest through the quick replies of the room. The core refuses a longer text, and the text is not a press.
