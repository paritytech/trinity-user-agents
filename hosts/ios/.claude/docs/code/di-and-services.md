# Dependency Injection & Services

## Overview

No DI framework (unlike Android's Hilt). Dependencies are wired manually via:
- **ServiceCoordinator** — central service orchestrator
- **DependencyLocator** — service location pattern
- **Wireframe** factories — VIPER module assembly

## ServiceCoordinator

`polkadot-app/Common/Services/ServiceCoordinator.swift`

Central hub managing the app's long-lived services. Created in MainTabBar module and passed to child modules as needed.

### Managed Services
- `balancesUpdatingService` — asset balance syncing
- `chatCoordinator`, `callCoordinator` — messaging & calls
- `signInHostCoordinator` — authentication
- `attachmentUploadService`, `attachmentDownloadService` — file handling
- `depositService` — deposit operations
- `fiatOnrampService`, `fiatOnrampTrackingService`, `fiatOnrampStorage` — fiat integration
- `polkadotHandshakeService` — protocol handshake
- `coinageService`, `coinageBackupSyncService` — digital identity
- `audioSessionManager` — audio management
- `determineStateSyncService` — state synchronization
- `accountManager` — product account management
- `allowanceManagerFacade` — transaction allowances

## Module Assembly (Wireframe)

Each VIPER module is assembled in its Wireframe:
```swift
static func createModule(/* dependencies */) -> UIViewController {
    let view = SomeViewController()
    let presenter = SomePresenter()
    let interactor = SomeInteractor(dependency: dependency)
    let wireframe = SomeWireframe()

    view.presenter = presenter
    presenter.view = view
    presenter.interactor = interactor
    presenter.wireframe = wireframe
    interactor.presenter = presenter

    return view
}
```

## Service Lifecycle Protocol

All managed services follow `ApplicationServiceProtocol`:

```swift
protocol ApplicationServiceProtocol {
    func setup()     // Initialize and start
    func throttle()  // Reduce activity (backgrounding)
}

protocol AsyncApplicationServicing {
    func setup() async
    func throttle() async
}
```

Services are set up on app launch via ServiceCoordinator, throttled when backgrounding.

## Hard Rules

1. **Services must be non-optional in coordinators** — if always expected, declare as non-optional. "I think we need always assume that service should be created successfully and pass non nil to coordinator in init." (PR review)
2. **No init{} side effects** — services should be passive until explicitly started
3. **Don't block users during startup** — separate async setup from required initialization
4. **Single responsibility per service** — don't inject unrelated concerns into existing services
5. **Use existing protocol abstractions for system services** — e.g., `UserNotificationServicing` instead of direct `UNUserNotificationCenter`
6. **Don't expose internal sub-services through protocol properties** — wire them internally during setup
7. **Don't create unnecessary provider/wrapper classes** — when the service can be configured directly
8. **Collapse single-purpose factories into services** — when a factory just wraps a network fetch and the service just calls it through, merge them
9. **Inject logger through init** — not via `Logger.shared` singleton inside methods
10. **Store StorageFacade reference for lazy repositories** — when a service may need different repository types at different times, store the facade and create repositories on demand rather than eagerly in init
11. **Prefer a protocol over a closure for injected dependencies** — model a collaborator as a named protocol with an implementation (e.g. `DateProviding` / `NowDateProvider`) rather than passing a bare `() -> T` closure. The protocol names the role, is discoverable and reusable, gives mocks a clear conformance point, and can grow additional methods without changing every call site. Reserve closures for genuinely one-off, single-call callbacks.

## App Initialization Flow

1. `AppDelegate.didFinishLaunching` — issue monitoring, background tasks, push registration, analytics
2. `SceneDelegate.scene(_:willConnectTo:)` — RootWindow, ThemeManager, TypographyManager, RootPresenter
3. `RootInteractor.setup()` — determines app state (onboarding, restore, dashboard, broken, jailbroken)
4. `MainTabBar` — initializes ServiceCoordinator, starts service subscriptions

### Build Configuration Guards
- Sentry must not run in unit tests or preview builds
- Guard with environment checks in `AppDelegate` initialization
- Build scripts excluded for Debug configuration
- Build phase scripts for symbol upload (Sentry dSYMs) must exclude Debug

## Seams

| Seam                     | Where                                    | When to touch                           |
|--------------------------|------------------------------------------|-----------------------------------------|
| ServiceCoordinator       | `Common/Services/ServiceCoordinator.swift` | Adding/removing managed services       |
| Wireframe factories      | `Modules/{Name}/{Name}Wireframe.swift`   | Module dependency changes               |
| DependencyLocator        | `Common/Services/DependencyLocator.swift`| Service location changes                |
| App startup              | `AppDelegate.swift`, `SceneDelegate.swift`| Initialization order changes           |
