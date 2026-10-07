---
title: "Host-drawn scanner"
owner: "@valentinfernandez1"
status: draft
---

# RFC — Host-drawn scanner

## Summary

A product asks the host to open its native QR and barcode viewfinder. The host draws the viewfinder, the user points it
at a code, and the product receives that code's text and format. The product never receives camera frames, needs no
camera permission, and the host does not act on what was scanned.

## Motivation

Every product that scans a code today builds its own viewfinder, so each one looks and behaves differently while both
native hosts already ship a polished scanner of their own. Scanning a receipt, a ticket or an address should feel the
same in every product.

A Worker execution has no camera at all, so a Pocket card cannot offer to scan anything. A page execution can call
`getUserMedia` after a `Camera` grant, but it then ships its own decoder and viewfinder, and it receives the whole video
stream for a task that needs one string.

## Approach

A new `Scanner` trait has one method. The product names the formats it accepts and, optionally, a prefix the text must
start with and a one-line hint. The host opens its viewfinder over whatever is on screen and closes it on the first code
that passes the filter. Codes that do not pass are ignored: the viewfinder stays open and the host briefly says the code
is not for this product.

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
    console.log("the user closed the viewfinder; worth offering again");
    break;
}
```

```rust
/// Code formats the host scanner reads.
pub enum CodeFormat {
    Qr, Aztec, DataMatrix, Pdf417,
    /// Also UPC-A, reported with a leading zero.
    Ean13,
    Ean8, UpcE, Code128, Code39, Code93, Itf,
}

pub struct HostScannerScanRequest {
    /// Formats the product accepts. At least one.
    pub formats: Vec<CodeFormat>,
    /// Exact, case-sensitive start the text must have. At most 256 bytes.
    pub prefix: Option<String>,
    /// One line of plain text shown under the host's title. At most 80 characters.
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
    /// No formats, or a prefix or hint over its limit.
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

`Dismissed` is worth offering again, `CameraUnavailable` is not until the user changes a setting, and `Unsupported`
(a host with no scanner) never is on that host. One code per call: a product that scans several calls again.
Cancelling the call closes the viewfinder ([Request cancellation](request-cancellation.md)).

**The host owns the viewfinder.** It writes the title itself, naming the requesting product by its id ("Scan for
greenmarket.dot"), and shows the product's hint below it as plain text. A product cannot draw over the viewfinder or
title it, so it cannot dress up a request as a host instruction such as "scan to sign in".

**The host does not act on what it scanned.** The host's own scanner treats some codes as deep links: a link to another
product opens it, a pairing link starts sign-in, a payment link opens a payment screen. A product's scan skips all of
that and returns the text. A product that wants to follow a scanned link calls `navigate_to`, under the rules that call
already has.

**No permission.** The product never touches the camera, and the user pointing the host's viewfinder at a code is the
consent. The host application's own OS camera prompt still applies and is shown inside the host's flow. The `Camera`
permission and `getUserMedia` are unchanged: a product that needs the camera for anything else, or wants its own
scanning UI, keeps doing so.

**The core checks both sides.** It refuses an invalid request before any viewfinder is raised, allows one open scan per
host, and refuses a host answer whose format the product did not accept or whose text lacks the prefix. The native hosts
filter codes in the viewfinder with the same rule, exported from the core, so iOS and Android accept exactly the same
codes. Scanning needs no signed-in session.

**Hosts.** iOS and Android draw the viewfinder with the platform decoders they already use (AVFoundation and ML Kit),
whose common formats are the list above. A web host answers `Unsupported`. The headless CLI host and the test host
answer with a code the test supplies, so a product can test its scan flow without a camera.

## Trade-offs

- Text only. A code whose content is not valid text is skipped like one that fails the filter. Raw bytes can be added
  later as an optional field without breaking existing callers.
- A prefix is the only content filter. A product that needs finer checks validates the text itself and calls again.
- Any execution can call it, including a Worker the user is not looking at. The title always names the product and the
  user can close it, which is the exposure the contact picker already has.
- Web hosts have no scanner yet, so a product keeps its own `getUserMedia` fallback there. A bundled decoder can be
  added later without changing this API.
- The host's main scanner still cannot hand a code to a product. A product claiming codes it handles is a separate
  feature.
- Dropped: a product-written title (lets a product pose as the host); a regular-expression filter (iOS, Android and
  browsers disagree on regex syntax); a live accept/reject exchange per code (more protocol for a need no product has
  yet); the host routing scanned deep links (a product's scan would leave the product); a `Camera` grant on `scan`
  (the product never receives camera data).
