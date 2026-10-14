# UI / UIKit Patterns

## Overview

UI is primarily programmatic UIKit with SwiftUI views hosted via `UIHostingController`. No Storyboards. Dark mode enforced app-wide. Legacy views use SnapKit for Auto Layout; new views use SwiftUI.

## Layout Architecture

### SwiftUI ViewLayout Pattern (Current)
- ViewControllers extend `UIHostingController<ModuleNameViewLayout>`
- ViewLayout is a SwiftUI `View` struct
- ViewModels use `@Observable` macro (Observation framework)
- ViewLayout lives in `Packages/PolkadotUI/Sources/Modules/`
- Events flow via closure callbacks on ViewModel

```swift
// ViewController wraps SwiftUI
final class SomeViewController: UIHostingController<SomeViewLayout> {
    var presenter: SomePresenterProtocol!
    
    convenience init() {
        self.init(rootView: SomeViewLayout())
    }
    
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(.bgSurfaceMain)
        presenter.setup()
    }
}

// ViewLayout is SwiftUI
struct SomeViewLayout: View {
    @State var viewModel: SomeViewModelProtocol = SomeViewModel()
    
    var body: some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                ForEach(viewModel.items, id: \.self) { item in
                    Button { viewModel.onTap?(item) } label: {
                        ItemRow(model: item)
                    }
                }
            }
        }
    }
}
```

### Legacy SnapKit Pattern
Older modules use UIView subclass with SnapKit constraints:
```swift
subview.snp.makeConstraints { make in
    make.top.equalToSuperview().offset(16)
    make.leading.trailing.equalToSuperview().inset(16)
}
```

## Design System

- **PolkadotUI** package — shared design system components
- **polkadot-app-design-system-ios** — external design system package with tokenized themes, typography, and radii
- **ThemeManager** — app-wide theme management (dark mode enforced)
- **TypographyManager** — typography system setup
- See `code/design-system.md` for full details

## UI Component Location

| Component type            | Location                                  |
|---------------------------|-------------------------------------------|
| Module-specific layout    | `PolkadotUI/Sources/Modules/`             |
| Shared UI components      | `polkadot-app/Common/View/`              |
| Design tokens/themes      | `PolkadotUI` + design-system package      |
| Reusable controls         | `Packages/UIKitExt/`                      |
| Image loading             | Kingfisher                                |
| Animations                | Lottie                                    |
| SVG rendering             | SVGKit                                    |
| QR codes                  | QRCode                                    |

## SwiftUI Conventions

### Color Shorthand
Use the shortest token-based color reference when type context allows:
```swift
// GOOD
Text("Hello").foregroundStyle(.fgPrimary)

// ACCEPTABLE
Color(.fgPrimary)

// AVOID (unnecessary verbosity)
Color.fgPrimary
```
(review: "I guess just `.fgPrimary` should work")

### State Transitions
Add opacity/crossfade transitions when swapping visual states:
```swift
// GOOD: Animated state change
view.transition(.opacity)
    .animation(.easeInOut, value: currentState)

// BAD: Abrupt visual switch with no transition
```
(review: "I think it would be nice to have opacity transition here")

### Stack View Insertion
Prefer natural ordering over manual index tracking:
```swift
// GOOD: Reverse order + insert at 0, or natural order + addArrangedSubview
// BAD: Manual index tracking with insertArrangedSubview
```

## Hard Rules

1. **No Storyboards** — all layout is programmatic
2. **Dark mode only** — `UIUserInterfaceStyle: Dark` in Info.plist; don't add light mode support
3. **Max line width 120** — enforced by SwiftFormat and SwiftLint
4. **Max method length 50 lines** — divide large methods into smaller ones
5. **Max nesting 3 levels** — extract deeply nested code
6. **Localize all strings** — use localization system; hardcoded strings are flagged in reviews
7. **Use design system tokens** — colors, spacing, typography from the design system, not hardcoded values
8. **Remove preview/debug Tasks before merging** — debug or preview-only Tasks must be clearly scoped or removed

## Cell Reuse Performance

Be mindful of expensive operations during UICollectionView/UITableView cell reuse:
- Cache results of expensive computations
- Don't make new network calls or hash resolutions on every dequeue

## Common Components

- `AccountPillView` — account display
- `AmountInput` (6 files) — amount entry
- `BalanceView` — balance display
- `ConfirmView` / `GenericConfirmView` — confirmation dialogs
- `LoadableRoundedButton` / `LoadableActionView` — loading state buttons
- `RecipientInputView` — recipient address entry
- `BottomSheet/` — sheet presentation (6 items)
- `PageSheet/` — modal presentation
- `QRScanner/` — QR code scanning (11 items)

## Avoid

- Don't create unnecessary wrapper abstractions when the underlying type suffices
- Don't add both `ThemeMode` and `ThemeSelection` if they serve the same purpose — "Why do we need both?"
- Don't create `Font` extensions when converting from `UIFont` directly works
- Method naming should reflect when/how it's called — "setup" for app launch initialization
