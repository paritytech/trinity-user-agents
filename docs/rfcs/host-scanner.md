---
title: "Host-drawn scanner"
owner: "@valentinfernandez1"
status: draft
---

# RFC — Host-drawn scanner

## Summary

A product asks the host to open its own QR and barcode viewfinder. The user points the camera at a code, and the product
gets back the code's text and format. The product never sees the camera, needs no camera permission, and the host does
not act on what was scanned.

## Motivation

Today every product that scans a code builds its own viewfinder. Each one looks and behaves differently, even though
both native hosts already ship a good scanner. Scanning a receipt, a ticket or an address should feel the same in every
product.

A Worker has no camera at all, so a Pocket card cannot offer to scan anything. A page can call `getUserMedia` after a
`Camera` grant, but then it has to ship its own decoder and viewfinder, and it receives the whole video stream when it
only needs one string.

## Approach

A new `Scanner` trait has one method, `scan`. The product says which formats it accepts. It can also give a prefix the
text must start with, and a one-line hint. The host opens its viewfinder and closes it on the first code that passes.
Codes that do not pass are ignored. The viewfinder stays open and the host briefly says the code is not for this
product.

```ts
const result = await truapi.scanner.scan({
  formats: ["Qr"],
  prefix: "https://greenmarket.example/r/",
  hint: "Point at the receipt's QR code",
});
assert(result.isOk(), "scanner.scan failed:", result);
const outcome = result.value.outcome;
switch (outcome.tag) {
  case "Scanned":
    console.log(outcome.value.format, outcome.value.text);
    break;
  case "Dismissed":
    console.log("the user closed the viewfinder, worth offering again");
    break;
}
```

```rust
/// Code formats the host scanner reads.
pub enum CodeFormat {
    Qr, Aztec, DataMatrix, Pdf417,
    /// Also UPC-A, reported with a leading zero.
    Ean13,
    Ean8, UpcE, Code128, Code39, Code93, Itf, Codabar,
}

pub struct HostScannerScanRequest {
    /// Formats the product accepts. At least one.
    pub formats: Vec<CodeFormat>,
    /// Start the text must have, ignoring ASCII letter case. At most 256 bytes of UTF-8.
    pub prefix: Option<String>,
    /// One line shown under the host's title. At most 80 Unicode scalar values.
    pub hint: Option<String>,
}

pub enum ScanOutcome {
    Scanned { text: String, format: CodeFormat },
    Dismissed,
}

pub struct HostScannerScanResponse {
    pub outcome: ScanOutcome,
}

pub enum HostScannerScanError {
    /// No camera, or the user refused the host application one.
    CameraUnavailable,
    /// Another scan is already open.
    Busy,
    /// The calling execution is not on screen, and is not a Worker handling a tap.
    NotVisible,
    /// No formats, or a prefix or hint that breaks its limit.
    InvalidRequest { reason: String },
    Unknown { reason: String },
}

#[wire_trait(id = 25)]
pub trait Scanner {
    #[wire(id = 0)]
    async fn scan(&self, cx: &CallContext, request: HostScannerScanRequest)
        -> Result<HostScannerScanResponse, CallError<HostScannerScanError>>;
}
```

What each answer means for the product:

| Answer | What the product does next |
| --- | --- |
| `Scanned` | Uses the text and format. |
| `Dismissed` | Can offer the scan again. |
| `CameraUnavailable` | Stops offering it until the user changes a setting. |
| `Busy` | Waits for the other scan to end. |
| `NotVisible` | Offers the scan from its own screen, or from a tap on its card. |
| `InvalidRequest` | Fixes the call. Nothing was shown. |
| `Unsupported` | Uses its own camera code. This host will never have a scanner. |

One call scans one code. A product that wants several calls again. Cancelling the call closes the viewfinder
([Request cancellation](request-cancellation.md)).

**The host owns the viewfinder.** The host writes the title itself and names the product by its id, for example "Scan
for greenmarket.dot". The product cannot draw over the viewfinder or change the title. The hint is shown below the
title as the product's own words, so the user can tell it apart from host text. To keep it to one plain line, a hint
may not contain control characters, line or paragraph separators, or characters that change text direction.

**The host does not act on what it scanned.** The host's own scanner treats some codes as links. A link to another
product opens it, a pairing link starts sign-in, and a payment link opens a payment screen. A product's scan skips all
of that and returns the text. If the product wants to follow a scanned link, it calls `navigate_to`, which keeps its own
rules.

**A pairing request never reaches a product.** A pairing link lets whoever answers it first pair with the device that
showed it, so it is a credential. The core refuses any code it would itself accept as a pairing request, whatever the
request's formats and prefix, including the bare handshake without the `pair` link around it. In the viewfinder it is
treated like any code that is not for this product. The prefix stays optional, because a barcode such as an EAN-13
grocery code has no prefix to give, and a required prefix would not help anyway: a product could pass
`polkadotapp://pair?` as its prefix.

**Only for what the user is looking at.** The viewfinder opens over whatever is on screen, so the call has to come from
there. An App or Widget may scan only while it is the screen the user sees; the host checks that. A Worker has no screen
of its own, so it may scan only within 5 seconds of the user tapping something the host drew for it, such as its card
face. Chat messages and actions do not count, because they can come from other people. Anything else is answered
`NotVisible` and no viewfinder opens. A product therefore cannot open the viewfinder from the background, or open it
again each time the user closes it.

**No permission.** The product never touches the camera, and the user pointing the viewfinder at a code is the consent.
The OS still asks the host application for camera access the first time. The `Camera` permission and `getUserMedia` do
not change. A product that needs the camera for anything else, or wants its own scanning screen, keeps using them.

**The core checks both sides.** Before any viewfinder opens, the core refuses an invalid request and allows only one
open scan per host. When the host answers, the core checks the code again. If its format was not asked for, or its text
lacks the prefix, the product gets `Unknown`, never the text. The native hosts filter codes in the viewfinder with the
same rule, taken from the core. A host reports a UPC-A code as EAN-13 with a leading zero before it checks the rule, so
iOS and Android accept the same codes. Scanning does not need a signed-in session.

**Text only.** The product gets the code's content as text. A code whose content the platform cannot read as text is
skipped, the same way as a code that fails the filter.

**Hosts.** iOS and Android draw the viewfinder with the decoders they already use, AVFoundation and ML Kit. The format
list is what both read. A web host answers `Unsupported`. The headless CLI host and the test host answer with a code the
test supplies, so a product can test its scan flow without a camera.

## Trade-offs

- A prefix is the only content filter. A product that needs finer checks reads the text itself and calls again.
- The prefix ignores ASCII letter case, because QR codes often store URLs in capitals. A product that cares about case
  checks the text itself.
- A product that gives no prefix can receive any code the user points the viewfinder at, apart from a pairing request.
  The title names the product, so pointing the camera is the user's choice, as with the contact picker.
- A Worker's 5-second window follows browsers' transient user activation. A Worker that needs longer between the tap
  and the scan is doing work the user is not waiting on, and should not scan.
- Raw bytes are not returned. They can be added later as an optional field without breaking existing callers.
- Web hosts have no scanner yet, so a product keeps its own `getUserMedia` fallback there. A bundled decoder can come
  later without changing this API.
- The host's main scanner still cannot hand a code to a product. Letting a product claim the codes it handles is a
  separate feature.
- Considered and dropped:
  - A title written by the product. It would let a product look like the host.
  - A regular expression filter. iOS, Android and browsers read regular expressions differently.
  - Accepting or rejecting each code live. It needs a two-way exchange that no product needs yet.
  - The host following scanned links. The user would leave the product mid-scan.
  - A `Camera` grant on `scan`. The product never receives camera data.
  - A required prefix. Barcodes have no prefix to give, and a product could pass a pairing link's start as its prefix.
