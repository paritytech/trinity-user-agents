# VIPER Architecture

## Overview

All feature screens in `polkadot-app/Modules/` follow the VIPER pattern. Every module has a mandatory 6-file structure. Use `./generate-viper-module.sh ModuleName` to scaffold new modules.

## Module Structure

```
Modules/{ModuleName}/
  {ModuleName}ViewController.swift   # UIHostingController<ViewLayout> — wraps SwiftUI in UIKit
  {ModuleName}ViewLayout.swift       # SwiftUI View — layout & subviews (lives in PolkadotUI package)
  {ModuleName}Presenter.swift        # Presentation logic, orchestrates View <-> Interactor
  {ModuleName}Interactor.swift       # Business logic, network calls, CoreData operations
  {ModuleName}Wireframe.swift        # Module assembly (creates & wires components) + navigation
  {ModuleName}Protocols.swift        # Protocol contracts between all layers
  {ModuleName}ViewFactory.swift      # View creation & configuration
  ViewModel/ (optional)
    {ModuleName}ViewModelProtocol.swift
    {ModuleName}CellViewModel.swift
    {ModuleName}ViewModelFactory.swift
```

Note: ViewLayout lives in `Packages/PolkadotUI/Sources/Modules/` when generated via the script.

### UIHostingController Pattern

ViewControllers wrap SwiftUI views using `UIHostingController`:

```swift
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
```

### @Observable ViewModel Pattern

ViewLayouts are SwiftUI Views using the `@Observable` macro (Observation framework):

```swift
struct SomeViewLayout: View {
    @State var viewModel: SomeViewModelProtocol = SomeViewModel()

    var body: some View {
        // SwiftUI layout using viewModel
    }
}

@Observable
final class SomeViewModel: SomeViewModelProtocol {
    var items: [CellViewModel] = []
    var onTap: ((CellViewModel) -> Void)?
}
```

Events flow from ViewLayout to Presenter via closure callbacks on the ViewModel.

## Data Flow

```
User tap → ViewController → Presenter → Interactor (business logic / network)
                                ↓
                          Presenter (formats data)
                                ↓
                          ViewController (updates UI via ViewLayout)
```

1. **ViewController** captures user input, calls Presenter methods
2. **Presenter** coordinates between View and Interactor; formats data for display
3. **Interactor** performs business logic, network calls, CoreData operations
4. Results flow back: Interactor → Presenter → ViewController
5. **Wireframe** handles navigation to other modules

## Protocol Contracts

Every module's `Protocols.swift` defines the boundaries:

```swift
protocol {ModuleName}ViewProtocol: ControllerBackedProtocol {
    // Presenter → View (UI updates)
}

protocol {ModuleName}PresenterProtocol: AnyObject {
    func setup()
    // View → Presenter (user actions)
}

protocol {ModuleName}InteractorInputProtocol: AnyObject {
    // Presenter → Interactor (business logic requests)
}

@MainActor
protocol {ModuleName}InteractorOutputProtocol: AnyObject {
    // Interactor → Presenter (results callbacks)
    func didReceive(result: Result<Model, Error>)
}

protocol {ModuleName}WireframeProtocol: AnyObject, AlertPresentable, ErrorPresentable {
    // Navigation actions
}
```

### Interactor → Presenter Communication

**Use `InteractorOutputProtocol` callback methods, NOT publishers/Combine.** When the Interactor needs to push data to the Presenter, add a callback method on the output protocol:

```swift
// GOOD: Protocol callback
@MainActor
protocol SomeInteractorOutputProtocol: AnyObject {
    func didReceive(status: AllowanceStatus)
}

// BAD: Exposing publisher on InteractorInputProtocol
protocol SomeInteractorInputProtocol {
    var allowanceStatus: AnyPublisher<Bool, Never> { get } // Don't do this
}
```

(review: "Let's use InteractorOutputProtocol and add callback function there")

## Base Class Inheritance

Presenters and Interactors can inherit from base classes to share common logic:

```swift
// Base class with shared token logic
class TokensPresenter {
    let tokensInteractor: TokensInputProtocol
    private(set) var chainAssets: [ChainAsset]?

    func didReceive(chainAssets: [ChainAsset]) {
        self.chainAssets = chainAssets
    }
}

// Concrete module extends base
class SelectTokenPresenter: TokensPresenter {
    override func didReceive(chainAssets: [ChainAsset]) {
        super.didReceive(chainAssets: chainAssets)
        provideViewModel()
    }
}
```

Use base class inheritance when multiple modules share the same data fetching or subscription logic (e.g., `TokensPresenter`, `TokensInteractor`).

## Hard Rules

1. **ViewController never talks to Interactor directly** — always through Presenter
2. **Interactor never imports UIKit** — pure business logic
3. **Wireframe is the only place that creates module components** — it wires all dependencies
4. **ViewLayout handles all layout code** — ViewController only manages lifecycle and event forwarding
5. **All inter-layer communication through protocols** — no concrete type references across layers
6. **Place private methods in separate Swift extensions** — keeps the main class body clean
7. **@MainActor on InteractorOutputProtocol** — but scope narrowly: don't mark entire Task as @MainActor, just `await` the presenter call
8. **WebView-based features must have proper VIPER modules** — even for WKWebView features, create a full module. Business logic and async tasks belong in the Interactor, not the Wireframe
9. **Use InteractorOutputProtocol callbacks, not publishers** — Interactor-to-Presenter data flow uses protocol callback methods, not Combine publishers

## Module Assembly Pattern

```swift
// In Wireframe:
static func createModule() -> UIViewController {
    let view = ModuleNameViewController()
    let presenter = ModuleNamePresenter()
    let interactor = ModuleNameInteractor()
    let wireframe = ModuleNameWireframe()

    view.presenter = presenter
    presenter.view = view
    presenter.interactor = interactor
    presenter.wireframe = wireframe
    interactor.presenter = presenter

    return view
}
```

## When to Create a New Module

Create a new VIPER module when:
- Adding a new screen/flow to the app
- The screen has its own navigation entry point

Do NOT create a module for:
- A reusable component (put in `Common/View/` or `PolkadotUI`)
- A bottom sheet or alert (use existing presentation patterns)
- Pure business logic (put in an Interactor or package)

## Common Mistakes from Reviews

- Putting layout code in ViewController instead of ViewLayout
- Skipping the Protocols file and using concrete types
- Making Interactor depend on UIKit types
- Creating overly large files — split when approaching 400 lines (reviewers flag this)
- Not using the generator script for new modules
- Exposing publishers on InteractorInputProtocol instead of using OutputProtocol callbacks
- Marking entire Task as @MainActor instead of just awaiting the presenter call
- Running async tasks in Wireframe instead of Interactor (especially for WebView features)
- Not inheriting from base classes when shared logic exists (e.g., TokensPresenter)

## Peer Files

When modifying a VIPER module, these files are often co-dependent:
- Changing a Presenter method → update Protocols.swift
- Adding a new Interactor method → update Protocols.swift + Presenter
- Changing ViewLayout → may need ViewController + ViewFactory updates
- Adding navigation → update Wireframe + Protocols
