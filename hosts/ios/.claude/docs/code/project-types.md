# Project Types & Utilities

## SubstrateSdk Types

Use these typed utilities from `substrate-sdk-ios` instead of raw primitives:

| Operation              | Use                                  | Don't use               |
|------------------------|--------------------------------------|-------------------------|
| Hex encoding           | `data.toHex()` (Data wrapper)        | `NSData.toHexString` from NovaCrypto, custom hex conversion |
| Hex with prefix        | `data.toHex(includePrefix: true)`    | Manual `0x` prepend     |
| Hex decoding           | `str.fromHex()` (SubstrateSdkExt)    | `Data(hexString:)` directly, `NSData(hexString:)` from NovaCrypto, custom hex parsing |
| Random data            | `Data.randomOrError`                 | `Data(random:)` etc.    |
| SCALE encoding         | `ScaleEncodable` conformance         | Manual byte packing     |
| SCALE decoding         | `ScaleDecodable` / `resultDecoder`   | Manual byte parsing     |
| Account IDs            | Typed AccountId                      | Raw String/Data         |

## HTTP Constants

Use existing typed constants:

```swift
HttpMethod.post          // not "POST" string
HttpContentType.json     // not "application/json"
HttpHeaderKey.contentType // not "Content-Type"
```

## Conversion Utilities

- `toAddressOrHex()` — address display with fallback (SubstrateSdkExt)
- `utf8View` — safe string-to-data conversion (avoids optionals)
- SCALE coding to/from `Data`: use `Type.fromScaleEncoded(data)` (SubstrateSdkExt) and
  `value.scaleEncoded()` — never create `ScaleDecoder(data:)` / `ScaleEncoder()` manually
  at call sites; manual coders are for `init(scaleDecoder:)` / `encode(scaleEncoder:)`
  implementations only
- Integer byte conversion: `littleEndianBytes` / `bigEndianBytes` on `UInt32`/`UInt64`
  (FoundationExt) — never inline `withUnsafeBytes(of:)` at call sites; extend the
  FoundationExt helpers if a width is missing

## Decodable Patterns

```swift
// GOOD: Protocol conformance
struct TxPayload: Decodable {
    // Use Decodable with map function of JSON
}

// GOOD: Generic for multiple account ID formats
struct TransactionPayload<Signer: Decodable>: Decodable { ... }

// BAD: Manual JSON decoding
let dict = json as? [String: Any]
let value = dict?["key"] as? String  // Don't do this
```

## ScaleEncodable Conformance

When a type needs SCALE encoding across multiple call sites, implement conformance at the type level:

```swift
// GOOD: Universal, reusable
extension UsernameReservationMessage: ScaleEncodable { ... }

// BAD: Ad-hoc encoding at each call site
```

## Type Aliases for Domain Concepts

Use typed aliases instead of raw primitives for domain-specific parameters:

```swift
// GOOD
func fetchProduct(by id: ProductId) -> Product
func resolveChain(by id: ChainModel.Id) -> ChainModel

// BAD
func fetchProduct(by id: String) -> Product
func resolveChain(by id: String) -> ChainModel
```

(review: "use ProductId alias instead of String")

## Parse at API Boundaries

When receiving data from JS bridge, host API, or external sources, parse into typed Swift models immediately:

```swift
// GOOD: Typed model at boundary
let request = try JSONDecoder().decode(ScheduledNotificationRequest.self, from: data)
processNotification(request)

// BAD: Raw dictionaries passed deeper
func handleNotification(params: [String: Any]) { ... }
```

## Key Derivation

- Use the `KeyDerivation` package — all keypair derivation lives there; never hand-roll
- Cryptographic key derivation is separate per role (wallet/chat/identity/alias/coinage)
- `SharedSecretDerivationDomain` must be per-feature, never reused across features
- `RootEntropyManager.shared` manages app-wide entropy and key material via Keychain + the `KeyDerivation` package
