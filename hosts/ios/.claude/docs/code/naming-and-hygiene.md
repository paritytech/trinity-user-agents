# Naming & Hygiene

## Naming Conventions

### VIPER Suffixes (Mandatory)
- `{Name}ViewController` — UIViewController
- `{Name}ViewLayout` — UIView for layout
- `{Name}Presenter` — presentation logic
- `{Name}Interactor` — business logic
- `{Name}Wireframe` — navigation & assembly
- `{Name}ViewFactory` — view creation
- `{Name}Protocols` — protocol contracts

### Protocol Naming
- Protocols end with `Protocol` (e.g., `StorageFacadeProtocol`, `ErrorPresentable`)
- VIPER protocols: `{Name}ViewProtocol`, `{Name}PresenterProtocol`, etc.

### Type Naming
- View subclasses: `*ViewLayout` or `*View`
- Factories: `*ViewFactory`, `*ViewModelFactory`, `*Factory`
- Services: descriptive names (not the Android `Service` suffix which is reserved)
- Names must be descriptive of actual purpose — "`AssetQueryType` might be more clear"

### Method Naming
- Name reflects when/how called: "setup" for initialization, "handle" for events
- Prefer semantic names: `toAddressOrHex()` not `convertAddress()`
- **"mark" vs "save"**: use "mark" for in-memory state transitions, "save"/"persist" for writing to storage (`markPendingOnboarding` not `savePendingOnboarding`)
- Private methods go in separate extensions

## Code Hygiene

### Remove Dead Code
- Delete unused methods, properties, and imports
- Remove debug/development leftovers before merging
- Remove preview/debug Tasks before merging
- Replace unreachable values with computed functions:
  ```swift
  // Bad: hardcoded delay table where some values are unreachable
  // Good: func retryDelaySeconds(forAttempt attempt: Int) -> Int { 1 << attempt }
  ```

### Statics-Only Types Get Converted to Enums

A lint pass auto-converts a class/struct whose main declaration body holds only static
members into a caseless `enum` (`convenience_type`) — even when instance methods exist
in a protocol-conformance extension. That breaks `Type()` instantiation used for
default-argument DI. When a type must be instantiable, keep at least one instance
member (or the protocol conformance itself) directly in the main declaration body.

### Private Extension Placement
When a type extension is only used by one consumer, make it a `private extension` in the consumer's file rather than adding it to the type's shared file:
```swift
// In GameDashboardTelemetryEmitter.swift:
private extension SomeType {
    func specificHelper() { ... }
}
```
(review: "Worth moving as a private extension as it is being used only there.")

### Comments
- Comments explain "why" not "what"
- No redundant documentation of obvious code
- Use TODO for pending integrations: `// TODO: Integrate TURN nodes from backend (@piroznoe)`
- KDoc/documentation on public protocol methods only

### Imports
- No inline fully-qualified names — always import
- SwiftFormat handles import sorting (currently disabled)

### Magic Numbers
- Extract to named constants
- Don't hardcode on-chain values (prices, scores) — derive from chain state

### Enum Associated Values
Don't build an enum case's associated value with an inline `.init(...)` — instantiate it as a
named local first, then wrap it in the case. Inline init hides the type and hurts readability
(review: "Inline init of the associated enum value is not readable. Prefer instantiate
associated value separately"):
```swift
// Bad
return .chunked(.init(
    metadata: metadata,
    lastChunkIndex: nil,
    downloadedBytes: 0
))

// Good
let state = DownloadEntry.ChunkedState(
    metadata: metadata,
    lastChunkIndex: nil,
    downloadedBytes: 0
)

return .chunked(state)
```
Single-parameter payloads that fit on one line (e.g. `.onProgress(Progress(loaded: 10, total: 20))`)
are fine when the type name is spelled out; the rule targets multi-line `.init` blocks nested
inside a case.

## Logging

- **SwiftyBeaver only** — no `print()`, no `NSLog`
- Level discipline:
  - `error` — failures requiring attention
  - `warning` — recoverable issues
  - `info` — state changes, lifecycle events
  - `debug` — development/investigation only
- **No PII** in logs (except public AccountId/pubkey)
- Sentry for crash monitoring and non-fatal errors

## SwiftLint Rules (`.swiftlint.yml`)
- Max nesting: 3 levels
- Max line width: 120 characters
- `id` allowed as identifier name
- Opt-in: `array_init`, `closure_spacing`, `empty_count`, `empty_string`, `first_where`, multiline formatting
- Excluded: `Package.swift`, `*.generated.swift`, test targets, `Packages/`

## SwiftFormat Rules (`.swiftformat`)
- Swift 5.10, 4-space indent, max 120 width
- Wrap arguments/parameters: `before-first`
- Trailing commas disabled
- Import sorting disabled

### Design System Spacing
When a spacing/layout value doesn't exist in the Design System, discuss with the design team — don't silently add `TODO: Should be in DS`.

## From PR Reviews

- "Get rid of it as it is not used" — remove unused code
- "leftover, removed" — clean up leftovers
- "this file is already quite big, makes sense to move it somewhere?" — split large files
- "Let's localize it" — all user-visible strings must be localized
- "Should use `markPendingOnboarding`" — "mark" for state, "save" for persistence
- "Worth moving as a private extension" — single-consumer extensions stay private and co-located
