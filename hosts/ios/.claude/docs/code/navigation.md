# Navigation

## Overview

Navigation is handled by Wireframes in the VIPER architecture. Each module's Wireframe owns navigation logic — presenting, pushing, and dismissing view controllers.

## Wireframe Pattern

```swift
protocol SomeModuleWireframeProtocol {
    func showDetail(from view: SomeViewProtocol, with model: DetailModel)
    func dismiss(from view: SomeViewProtocol)
}

class SomeModuleWireframe: SomeModuleWireframeProtocol {
    func showDetail(from view: SomeViewProtocol, with model: DetailModel) {
        guard let viewController = view as? UIViewController else { return }
        let detailVC = DetailWireframe.createModule(model: model)
        viewController.navigationController?.pushViewController(detailVC, animated: true)
    }
}
```

## Navigation Patterns

### Tab Navigation
- **MainTabBar** — container for 4 tabs: Chat, Wallet, Browse, Settings
- Each tab has its own navigation stack

### Modal Presentation
- `BottomSheet/` (6 items) — sheet presentation
- `PageSheet/` — modal presentation
- Use existing presentation patterns before creating new ones

### Deep Links
Defined in `AppConfig/AppConfig.swift`:
- Chat, fiat onramp
- URL scheme-based deep linking
- `URLHandling` module handles incoming URLs

## Hard Rules

1. **Only Wireframes handle navigation** — ViewControllers and Presenters never push/present directly
2. **Semantic method names** — `showDetail`, `dismiss`, not generic `navigate`
3. **Deep link safety** — don't drop navigation when UI hierarchy isn't ready; queue or defer
4. **Use existing presentation patterns** — BottomSheet, PageSheet before creating custom
5. **URL-triggered navigation through ModuleNavigator** — when a URL/deep link triggers navigation, route through `ModuleNavigator`, not inline in the service that parses the URL
6. **Extract shared presentation logic to wireframe extensions** — when multiple wireframes present the same UI pattern (e.g., privacy degradation alerts), extract to a shared wireframe extension for reuse

## Deep Link Handling

```
URLHandling module → AppConfig deep link matching → Target module Wireframe
```

- Deep links must be safe when the tab bar isn't ready yet
- Product deep links go through the Products module

### Deeplink Pipeline

Three producers feed URLs into `DeferredLinkHandler` → `URLHandlingService` (chain of
`URLHandlingServiceProtocol` children matched by host):

1. **OS routing** — `SceneDelegate` (`openURLContexts` / user activities); only the
   active build scheme (`$(DEEPLINK)` in Info.plist) can arrive here
2. **Push notifications** — `DeeplinkPushRouteHandler` passes payload URLs directly,
   bypassing OS scheme routing
3. **QR scan** — `WalletQRScanResultHandler` re-enters via `UIApplication.shared.open`,
   so scanned URLs go through OS routing (path 1)

Rules:

- The active scheme is `AppConfig.DeepLink.scheme` (`polkadotapp` prod,
  `polkadotappdev` dev); all flavor schemes live in `AppConfig.DeepLink.knownSchemes` —
  never hardcode scheme strings elsewhere
- Cross-flavor links (e.g. a prod desktop pairing QR on a dev build) are rewritten to
  the active scheme by `DeeplinkSchemeNormalizer` at the QR seam before
  `UIApplication.shared.open`
- URL handlers must either be scheme-agnostic (match on host, like
  `PolkadotSignInService`) or validate against `AppConfig.DeepLink.scheme` — never a
  hardcoded flavor scheme
- Caveat: push-delivered URLs (path 2) skip normalization; if a scheme-validating flow
  ever arrives via push, move normalization into `DeferredLinkHandler.handle(with:)`

## Seams

| Seam                    | Where                                   | When to touch                      |
|-------------------------|-----------------------------------------|------------------------------------|
| Wireframe protocol      | `Modules/{Name}/{Name}Protocols.swift`  | Adding navigation actions          |
| Wireframe impl          | `Modules/{Name}/{Name}Wireframe.swift`  | Navigation implementation          |
| Deep link config        | `AppConfig/AppConfig.swift`             | New deep link routes               |
| URL handling            | `Modules/URLHandling/`                  | URL parsing/routing changes        |
| MainTabBar              | `Modules/MainTabBar/`                   | Tab structure changes              |
