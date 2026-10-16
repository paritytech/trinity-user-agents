---
title: "Host share sheet"
owner: "Leonardo Custodio"
status: draft
---

# RFC — Host share sheet

## Summary

A product asks the host to open its own share sheet with one text. The user chooses where the text goes: another
application, a chat, the clipboard. The product learns only whether the user shared the text, never where.

## Motivation

A product that wants people to invite others, a chat product with a contact link for example, has no way to hand a
text to the platform's share sheet. A page can call `navigator.share` in a web host, but a Worker has no page, and a
native host does not give its sheet to a product. The product can only show the link and ask the user to copy it.

## Approach

A new `Share` trait has one method, `share`. The host opens its sheet with the text as the only item and answers how
the sheet ended.

```ts
const result = await truapi.share.share({ text: "Add me: https://greenmarket.dot.li/r/42" });
assert(result.isOk(), "share.share failed:", result);
console.log(result.value.outcome); // "Shared" or "Dismissed"
```

```rust
pub struct HostShareRequest {
    /// At most 8 KiB of UTF-8, with no control characters other than line breaks and tabs.
    pub text: String,
}

pub enum ShareOutcome { Shared, Dismissed }

pub struct HostShareResponse { pub outcome: ShareOutcome }

pub enum HostShareError {
    /// Another share sheet is open.
    Busy,
    /// The calling execution is not on screen, and is not a Worker handling a press.
    NotVisible,
    /// An empty text, a text above the bound, or a control character.
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

#[wire_trait(id = 26)]
pub trait Share {
    #[wire(id = 0)]
    async fn share(&self, cx: &CallContext, request: HostShareRequest)
        -> Result<HostShareResponse, CallError<HostShareError>>;
}
```

| Answer           | What the product does next                                     |
| ---------------- | -------------------------------------------------------------- |
| `Shared`         | Nothing: the text went where the user chose.                   |
| `Dismissed`      | Can offer the share again.                                     |
| `Busy`           | Waits for the other sheet to close.                            |
| `NotVisible`     | Offers the share from its own screen, or from a press.         |
| `InvalidRequest` | Fixes the call. Nothing was shown.                             |
| `Unsupported`    | Shows the text so the user can copy it. This host has no sheet. |

**The host owns the sheet.** The host draws the platform sheet with the text as the only item, and never sends the
text anywhere by itself. Cancelling the call closes the sheet and answers `Dismissed`.

**Only for what the user is looking at.** An App or Widget may share only while it is the screen the user sees. A Worker
has no screen, so it may share only within 5 seconds of a press of the user on something the host drew for it: a
renderer action of one of its bodies, or a button, a menu entry, a quick reply or a command of one of its chat rooms. A
posted message does not count. The scanner keeps its stricter rule, because a viewfinder that opens by surprise shows
the camera, and a share sheet sends nothing until the user picks a target.

**One sheet.** The core allows one open share sheet per host and answers `Busy` to a second.

**No permission.** The user picks the target, so the choice is the consent, as with the contact picker and the scanner.

**Hosts.** The iOS app draws `UIActivityViewController`. Android, the web hosts and the CLI host roles have no sheet yet
and answer `Unsupported`; a product falls back to a text the user can copy.

## Trade-offs

- Text only. An image, a QR code for example, would need a resource type and a size bound. It can be added later as an
  optional field.
- The product does not learn the target. A product that wants to know whether its invitation arrived reads its own
  protocol, not the sheet.
- Chat presses count for the share sheet but not for the scanner. The difference follows what each surface exposes when
  it opens unasked.
