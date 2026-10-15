# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Polkadot iOS — a production-grade iOS wallet and social app for the Polkadot blockchain ecosystem. Features include wallet management, cross-chain transfers (XCM), real-time chat with WebRTC calls, fiat on-ramp, identity/username claiming, and QR-based interactions.

### Key Technologies
- **UIKit** — Primary UI framework, programmatic layout (no Storyboards)
- **SnapKit** — Auto Layout constraints
- **VIPER** — Architecture pattern for all feature modules
- **Swift Package Manager** — local packages under `Packages/`
- **substrate-sdk-ios** — Substrate/Polkadot blockchain interaction
- **Operation-iOS** (3.0.0) — Core Data service (writer/observer/reader modes), repositories, operations
- **CoreData** — Local persistence (SubstrateDataModel + UserDataModel)
- **WebRTC** — Peer-to-peer voice/video calls
- **Firebase** — Remote Config
- **Fastlane + GitHub Actions** — CI/CD pipeline

## Build Commands

Prefer the `xcode-tools` MCP server (XcodeBuildMCP) for builds, tests, and simulator control — it produces
errors/warnings-only output and reuses booted simulators. Use the raw `xcodebuild` commands below as fallback.

### Brand Configuration

`Configs/brand.xcconfig` is committed. A fresh clone builds with no bootstrap step. `Configs/brand.template.xcconfig` documents the key contract with placeholder values and is not read by the build.

Deleting or renaming `Configs/brand.xcconfig` fails the Xcode build at config parse:
```
Configs/base.debug.xcconfig:7: error: could not find included file 'brand.xcconfig' in search paths
```
The build action fails before any test runs, so there is no host-app crash to diagnose — the error names the file and stops at config parse.

A key declared in the template but absent from `brand.xcconfig` is **not** caught by the build: it expands to an empty string in Info.plist and traps at app launch in `AppConfig.Brand`. Add new keys to both files in the same commit.

### Build the App
```bash
# Xcode build (scheme: polkadot-app, target: iOS 17.0+)
set -o pipefail && xcodebuild -project polkadot-app.xcodeproj -scheme polkadot-app \
  -configuration Debug -destination 'platform=iOS Simulator,name=iPhone 16' \
  build 2>&1 | xcbeautify --quiet
```

### Run Tests

Always prefer running only the tests related to the change via `-only-testing:`; run the full suite
only before handoff or when explicitly asked.

```bash
# Targeted unit tests (fastest — preferred during development)
set -o pipefail && xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appTests \
  -destination 'platform=iOS Simulator,name=iPhone 16' \
  -only-testing:polkadot-appTests/SomeTestCase 2>&1 | xcbeautify --quiet

# Full unit test suite
set -o pipefail && xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appTests \
  -destination 'platform=iOS Simulator,name=iPhone 16' 2>&1 | xcbeautify --quiet

# Integration tests
set -o pipefail && xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appIntegrationTests \
  -destination 'platform=iOS Simulator,name=iPhone 16' 2>&1 | xcbeautify --quiet
```

### Generate a VIPER Module
```bash
./generate-viper-module.sh ModuleName
```

### Lint & Format
```bash
swiftlint lint
swiftformat .
```

## Architecture

### VIPER Module Structure

Each feature module in `polkadot-app/Modules/` follows VIPER:

```
{ModuleName}/
├── {ModuleName}ViewController.swift   # UIViewController — UI events
├── {ModuleName}ViewLayout.swift       # UIView — layout & subviews
├── {ModuleName}Presenter.swift        # Presentation logic
├── {ModuleName}Interactor.swift       # Business logic & data
├── {ModuleName}Wireframe.swift        # Navigation & module assembly
├── {ModuleName}Protocols.swift        # Contracts between layers
└── {ModuleName}ViewFactory.swift      # View creation & configuration
```

Use `./generate-viper-module.sh ModuleName` to scaffold new modules from Stencil templates in `swiftgen-templates/viper/`.

### Feature Modules (34 modules)

Separate screens (VIPER) modules are located in the `Modules/` directory.

### Local Packages (`Packages/`)

Local Packages are located in the `Packages` director. AppDependecy is a root package that defines all dependencies for the app.

### Data Flow
1. **Wireframe** creates and wires module components
2. **ViewController** captures user input, delegates to **Presenter**
3. **Presenter** coordinates between View and **Interactor**
4. **Interactor** performs business logic, network calls, and CoreData operations
5. Results flow back through Presenter to update the View

### Networking
- NetworkOperation from Operation-iOS or URLSession for HTTP requests.
- substrate-sdk-ios for Substrate storage subscription, query, JSON-RPC or state calls
- Operation-iOS library for chain of operations. Prefer to use structure concurrency instead of operations. One can also bridge operation in structured concurrency context, for example, using asyncExecute. Checkout StructuredConcurrency package for more bridging options. 
- AsyncExtensions for reactive streams. Prefer to use stream wrappers from StructuredConcurrency package instead of legacy subcriber/observer approach.

### Data Persistence
- **CoreData**: `SubstrateDataStorageFacade` (blockchain data), `UserDataStorageFacade` (user data)
- Prefer StorageFacadeProtocol.subscribeSnapshot and StorageFacadeProtocol.subscribeSingle for creating subscription streams for Core Data items
- **Repository pattern**: `CoreDataRepository<T, U>` with `CoreDataCodable` protocol
- **UserDefaults**: session and preferences. Prefer to use SettingsManager instead of using UserDefauls directly.
- **iCloud**: CloudBackup module for data synchronization

NOTES:

- When modifying part of the existing entity in the Core Data prefer to create a separate mapper instead of fetching the whole model first, modifying it and saving back. Separate mapper approach prevents race conditions.

## Testing

- **Unit tests**: `polkadot-appTests/` 
- **Integration tests**: `polkadot-appIntegrationTests/`
- **Test plan**: `polkadot-app.xctestplan`
- Use Swift Testing (`import Testing`, `@Test`, `#expect`/`#require`) for all new and
  rewritten tests; do not add new XCTest cases. See `.claude/docs/code/testing.md`

## Code Style

### SwiftLint (`.swiftlint.yml`)
- Max nesting: 3 levels
- Avoid generating lines of code over 120 symbols
- Avoid generating methods over 50 lines of code. Try to divide big methods into smaller one.
- `id` allowed as identifier name
- Opt-in rules: `array_init`, `closure_spacing`, `empty_count`, `empty_string`, `first_where`, multiline formatting rules
- Excluded: `Package.swift`, `*.generated.swift`, test targets, `Packages/`

### SwiftFormat (`.swiftformat`)
- Swift 5.10, max line width 120, indent 4 spaces
- Wrap arguments/parameters: `before-first`
- Imports sorting disabled
- Trailing commas disabled

### Naming Conventions
- VIPER suffixes: `ViewController`, `Presenter`, `Interactor`, `Wireframe`, `ViewFactory`, `Protocols`
- Protocols end with `Protocol`
- View subclasses: `*ViewLayout` or `*View`
- Factories: `*ViewFactory`, `ViewModelFactory`, `*Factory`

### Function Conventions

- Place private methods to the separate Swift extension

## Dependencies

Key external packages (via SPM):
- **substrate-sdk-ios** — Polkadot/Substrate blockchain SDK
- **ExtrinsicService** — Extrinsic construction and submission
- **Firebase SDK** — Remote Config
- **SnapKit** — Auto Layout DSL
- **Kingfisher** — Image loading and caching
- **Lottie** — Animations
- **WebRTC** — Real-time communication
- **SVGKit** — SVG rendering
- **SwiftyBeaver** — Logging
- **QRCode** — QR code generation

NOTES:

- Prevent to build local packages separately as it introduces .build file that increases the size of the app. Alternatively remove .build directories inside packages after build completes.

## Development Notes

- Keep comments minimal and purposeful. Use them to explain classes, protocols, public APIs, architectural decisions, or non-obvious logic. Avoid comments that restate what the code already expresses clearly
- Keep function bodies compact and focused. Prefer functions under 50 lines and split complex logic into smaller private helpers when readability starts to decline
- Limit function signatures to a maximum of 5 parameters. When more data is required, group related values into dedicated models, configuration objects, or context structures
- Write self-explanatory code. Favor clear naming, small abstractions, and straightforward control flow so that comments inside function bodies are rarely needed
- Use meaningful variable names instead of single-letter names, for example currentPayment instead of p.
- Use struct for data models and state, and use class for services or entities that contain complex logic.
- Dark mode enforced app-wide (`UIUserInterfaceStyle: Dark` in Info.plist)
- Shake gesture on `RootWindow` opens Debug Settings in development
- Minimum deployment target: iOS 17.0
- Swift version: 5.0 (project), 5.10–6.0 (packages)
- Build configurations: Debug, DevCI, Nightly, Release
- Push notifications via APNs + PushKit (VoIP)
- NotificationServiceExtension for rich push notifications
- Legacy code lives in `polkadot-app/Inherited/` — being gradually migrated
- `polkadot-app/Common/` contains shared utilities, services, and base classes
- Prefer to declare and throw an error instead of force unwrapping optionals
- use Data.randomOrError from SubstrateSdk for random and test data generation
- prefer toHex() from SubstrateSdk to convert to hex, and `hexString.fromHex()` from SubstrateSdkExt to convert back — it reads left to right at the call site and throws like `Data(hexString:)`, which it wraps. Use toHex(includePrefix: true) to add 0x prefix. hexString.withoutHexPrefix() to exclude 0x prefix.
- prefer depending on protocols rather than concrete implementations. Inject dependencies from the outside instead of creating them internally, even when the implementation is a singleton.
- **Keep comments minimal — code should be self-descriptive.** Prefer clear names
  over narration; don't restate what the code does or describe behavior that tests
  already cover. Reserve comments for non-obvious *why* (rationale, invariants,
  gotchas).

## Tools

Use tools in this priority order for iOS Swift Xcode projects:

1. **xcode-tools** — default first choice for Xcode-related work; use for project/workspace inspection, schemes, targets, configurations, build context, and Xcode-native operations; prefer for most implementation, debugging, and project setup tasks

2. **apple-docs** — use only when official Apple API semantics are unclear; use for framework behavior, lifecycle rules, availability, deprecations, Info.plist keys, entitlements, and correct API usage; do not use if xcode-tools already gives enough project-specific evidence

3. **xcstrings-crud** — use for any .xcstrings change; prefer over manual editing for localization catalogs

Rules:
- Prefer xcode-tools by default
- Use apple-docs only for framework truth, not project truth
- Do not guess Apple APIs when documentation is needed
- After meaningful code changes, validate through xcode-tools/Xcode-native flow
- Always check whether xcstrings-crud available via mcp or command line to edit .xcstrings. It can do it safely and efficiently. Check that extractionState is set to manual for each added string.
- Xcode writes `.xcstrings` JSON with space before every colon (`"key" : "value"`, 2-space indent).
  Any tool editing catalogs must match this format; `json.dump`/`JSONEncoder` bloats diffs.
  Verify the diff after editing.
- Always check whether figma mcp server is available in local settings. Prefer figma mcp to undertand layers and styles when a user asks for layout implementation from Figma mockups.
