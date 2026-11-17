# Humble Objects in VIPER

A humble object sits at a boundary that is hard to test (UIKit, SwiftUI, navigation, Core Data
managed objects, system frameworks). It is made so simple that it needs no test: it only moves
data between the boundary and a testable object. Any decision it contains is moved out.
(Meszaros, *xUnit Test Patterns*; Feathers, *The Humble Dialog Box*; Martin, *Clean
Architecture* ch. 23.)

## The rule

> If you want to test something inside a humble object, the test is telling you that logic is
> in the wrong place. Move the logic, test the new type, leave the humble object untested.

## What counts as humble here

- `*ViewController`, `*ViewLayout`, SwiftUI `View` structs (incl. PolkadotUI), cells,
  `UIHostingController` wrappers
- `*Wireframe`, `*ViewFactory` (assembly and navigation only)
- Core Data mappers that copy fields
- Thin adapters over system APIs (`BGTaskSchedulerAdapter`, notification center, keychain
  shims): test the policy that drives them, not the adapter

## Smells that mean logic is trapped in a humble object

| Smell in a humble object | Extract to | Test |
|--------------------------|------------|------|
| `if`/`switch` choosing text, color, typography, visibility from model state | `*ViewModel` value built by a pure `*ViewModelFactory` or a static `func style(for:)` | Parameterized table over states |
| Arithmetic (elapsed time, progress, amounts, thresholds) | Pure function or value type | Boundaries and rounding |
| `Task.sleep` / timers to sequence UI (minimum loading duration, debounce, reveal delays) | A small `@MainActor` state machine or policy type taking `any Clock<Duration>`; the view only renders its output | Drive with `ManualClock` |
| Deciding *whether* to navigate | Presenter/Interactor decision returning an enum route | Assert the route; Wireframe stays humble |
| Mapper filling defaults or interpreting missing data | Domain initializer or explicit rule type | Rule cases only |
| ViewController observing streams and combining them | Presenter or a reducer `(State, Event) -> State` | Event sequences |

## Recipe: view state that a test wants to read off labels

Before (what a test would have to inspect `amountLabel.text`, `typography`, `isHidden`):

```swift
final class ChatTransferMessageView: UIView {
    func apply(_ configuration: Configuration) {
        amountView.typography = configuration.amount.count > 6 ? .titleExtraLarge : ...
        originalAmountLabel.isHidden = configuration.originalAmountText == nil
        subtitleLabel.text = configuration.state == .claimed && configuration.originalAmountText != nil
            ? String(localized: .transferStatusAmountDiffers) : ...
    }
}
```

After:

```swift
struct TransferMessageViewModel: Equatable {
    let amountTypography: Typography
    let originalAmount: String?
    let subtitle: Subtitle   // enum, not a localized string
}

enum TransferMessageViewModelFactory {
    static func make(from message: TransferMessage) -> TransferMessageViewModel { ... }
}

// View: assignments only, no branches.
func apply(_ viewModel: TransferMessageViewModel) {
    amountView.typography = viewModel.amountTypography
    originalAmountLabel.isHidden = viewModel.originalAmount == nil
    originalAmountLabel.attributedText = viewModel.originalAmount.map(Self.strikethrough)
    subtitleLabel.text = viewModel.subtitle.localized
}
```

Test:

```swift
@Test(arguments: [
    ("20", Typography.headlineLarge),
    ("20,000", .headlineSmall),
    ("2,000,000", .titleExtraLarge)
])
func amountTypographyStepsDownAsAmountGrows(amount: String, expected: Typography) {
    let viewModel = TransferMessageViewModelFactory.make(from: .stub(amount: amount))
    #expect(viewModel.amountTypography == expected)
}
```

Keep semantic enums (`Subtitle.amountDiffers`) in the view model and resolve localization in the
view, so tests do not depend on strings.

## Recipe: UI timing (minimum loading duration)

`SPAViewController` computes `minimumLoadingDuration - elapsed` and calls `Task.sleep`. Extract:

```swift
@MainActor
final class MinimumDurationGate {
    private let clock: any Clock<Duration>
    private let minimum: Duration
    private var start: ContinuousClock.Instant?   // or the injected clock's instant

    func finish() async { /* sleep(for: remaining) on the injected clock */ }
}
```

The view controller calls `await gate.finish()` and renders. Test the gate with a clock that can
advance `now` (see time-and-async.md, ManualClock limits).

## What NOT to do

- Do not snapshot-test or inspect subviews to "cover" a view. If visual regressions matter for
  a design-system component, use a separate opt-in snapshot target that never runs in PR CI.
- Do not add `internal` accessors to views just so tests can reach labels.
- Do not test that a Wireframe pushed a controller; test the decision that chose the route.
