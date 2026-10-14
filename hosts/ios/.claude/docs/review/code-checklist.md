# Code Review Checklist

File-by-file code review. For each rule violated, note severity (blocking/major/minor), quote file:line, and suggest a concrete fix.

## Error Handling

- [ ] No force unwraps — use guard/throw instead
- [ ] No silent fallbacks to raw data or defaults — throw explicit errors
- [ ] Typed error enums per domain
- [ ] ErrorPresentable used for user-facing errors
- [ ] SwiftyBeaver for logging (not print/NSLog)
- [ ] Appropriate log levels (error/warning/info/debug)
- [ ] No PII in logs

**Ref:** code/error-handling.md

## Concurrency

- [ ] New code uses async/await (not Operation-iOS for new paths)
- [ ] Legacy Operation-iOS bridged via `asyncExecute` when touched
- [ ] `withRetry` from StructuredConcurrency for retries
- [ ] `AsyncPassthroughSubject` or `AsyncStream.makeStream` for streams (not custom continuation hacks)
- [ ] Tasks cancelled when new responses replace previous ones
- [ ] @MainActor scoped narrowly — on OutputProtocol, not entire Tasks
- [ ] No blocking startup on network when cached data available
- [ ] Retry delays computed (not hardcoded unreachable values)
- [ ] Debounce added for frequently called network endpoints
- [ ] Critical init call sites preserved when restructuring flows
- [ ] New long-running user-visible operations wrapped in `markStallRegion` or expose `StallActivitySource` if they already track work; multi-phase operations get a region per phase
- [ ] Methods calling instrumented code carry a doc comment noting progress reporting into the task-local staleness collector
- [ ] No `Task.detached` inside instrumented code (drops task-local collector); if unavoidable, rebind the collector explicitly inside
- [ ] Region titles localized at the call site

**Ref:** code/concurrency.md

## UI / UIKit / SwiftUI

- [ ] Programmatic layout only (no Storyboards)
- [ ] Layout code in ViewLayout (not ViewController)
- [ ] UIHostingController pattern for SwiftUI views
- [ ] @Observable ViewModels (not @StateObject/Combine)
- [ ] SwiftUI color shorthand (`.fgPrimary` not `Color(.fgPrimary)`)
- [ ] Opacity/crossfade transitions for visual state changes
- [ ] Design system tokens for colors/spacing/typography
- [ ] Missing DS values discussed with design team (not just TODO)
- [ ] No light mode code (dark mode enforced)
- [ ] Cell reuse: no expensive operations on dequeue
- [ ] Methods under 50 lines
- [ ] Nesting under 3 levels
- [ ] Lines under 120 characters
- [ ] No unnecessary abstraction wrappers
- [ ] Preview/debug Tasks removed before merge
- [ ] Stack view insertions use natural ordering (no manual index tracking)

**Ref:** code/ui-uikit.md, code/design-system.md

## Naming & Hygiene

- [ ] VIPER suffixes correct (ViewController, Presenter, Interactor, Wireframe, ViewFactory, Protocols)
- [ ] Protocols end with Protocol
- [ ] Names descriptive of actual purpose
- [ ] Method names reflect when/how called ("mark" for state, "save" for persistence)
- [ ] Dead code removed (unused methods, properties, imports)
- [ ] Debug/preview leftovers cleaned up
- [ ] TODO comments for pending integrations (with owner)
- [ ] All user-visible strings localized
- [ ] No magic numbers (extract to constants)
- [ ] Comments explain "why" not "what"
- [ ] Single-consumer extensions are private and co-located with consumer
- [ ] Type aliases used for domain concepts (ProductId not String)
- [ ] No unused parameters passed to functions
- [ ] Configuration URLs in AppConfig (not hardcoded)
- [ ] Enum associated values built as named locals, not multi-line inline `.init(...)` inside the case

**Ref:** code/naming-and-hygiene.md

## DI & Services

- [ ] Services non-optional in coordinator inits
- [ ] Module assembly in Wireframe only
- [ ] No unrelated concerns injected into services
- [ ] Sentry guarded in test/preview builds
- [ ] Internal sub-services not exposed through protocol properties
- [ ] Existing protocol abstractions used (not direct framework types)
- [ ] Features disabled at factory level (not early returns inside feature)
- [ ] No unnecessary provider/wrapper/factory indirections

**Ref:** code/di-and-services.md

## Data Persistence

- [ ] Separate mappers for partial CoreData updates
- [ ] SettingsManager for UserDefaults (not direct access)
- [ ] New version migration for schema changes
- [ ] Repository pattern for data access

**Ref:** code/data-persistence.md

## Types & SDK Usage

- [ ] `Data.toHex()` / `String.fromHex()` for hex conversion (not `Data(hexString:)` directly, nor the underlying `NSData` variants from NovaCrypto)
- [ ] `Data.randomOrError` for test data
- [ ] Decodable conformance (not manual JSON parsing)
- [ ] Generic models where type variations exist
- [ ] ScaleEncodable/ScaleDecodable at type level for reusable types
- [ ] HTTP typed constants (HttpMethod, HttpContentType, HttpHeaderKey)
- [ ] `fromScaleEncoded` for SDK decoding (not custom helpers)

**Ref:** code/project-types.md

## Navigation

- [ ] Navigation only in Wireframes
- [ ] Semantic method names (showDetail, not navigate)
- [ ] Deep links safe when UI not ready (queue/defer)
- [ ] Existing presentation patterns used (BottomSheet, PageSheet)
- [ ] URL-triggered navigation routes through ModuleNavigator
- [ ] Shared presentation logic in wireframe extensions (not duplicated)
- [ ] Notifications: both cancel pending AND remove delivered

**Ref:** code/navigation.md, code/error-handling.md

## Testing

- [ ] No private per-suite mocks — shared doubles in `Helpers/Mocks/` (universal) or `<Feature>/Mocks/` (feature), one type per file
- [ ] Existing shared mock extended instead of forked
- [ ] Mocks are dumb (record inputs, `simulate…` helpers); assertions live in the test
- [ ] Mocked at the lowest injectable seam, real production types exercised above it

**Ref:** code/testing.md

## PR Hygiene

- [ ] No unrelated changes in the diff
- [ ] Large files split if over 400 lines
- [ ] Peer files updated (Protocols.swift when adding methods, etc.)
- [ ] Consistent with surrounding code style

## Comment Style

When leaving review comments:
```
**[severity]** `file:line`

Description of the issue.

**Fix:** Concrete suggestion or code example.

*Ref: checklist-section*
```

## Verdict Format

```
## Code Review

**Verdict:** X blocking / Y major / Z minor

### Blocking
- [file:line] Description. Fix: ...

### Major
- [file:line] Description. Fix: ...

### Minor
- [file:line] Description. Suggestion: ...
```
