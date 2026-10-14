# VIPER Module Generation with SwiftGen Templates

This directory contains SwiftGen-compatible Stencil templates for generating VIPER modules.

## Overview

The legacy `generamba-module.sh` script has been replaced with `generate-viper-module.sh`, which uses SwiftGen-style Stencil templates for code generation.

## Usage

To generate a new VIPER module, run:

```bash
./generate-viper-module.sh ModuleName
```

For example:

```bash
./generate-viper-module.sh UserProfile
```

This will create a new module directory at `polkadot-app/Modules/UserProfile/` with the following files:

- `UserProfileInteractor.swift`
- `UserProfilePresenter.swift`
- `UserProfileProtocols.swift`
- `UserProfileViewController.swift`
- `UserProfileViewFactory.swift`
- `UserProfileWireframe.swift`

And in `Packages/PolkadotUI/Sources/Modules/UserProfile/`:
- `UserProfileViewLayout.swift` (SwiftUI view with preview)
- `UserProfileViewModel.swift` (@Observable class)

## Template Structure

The templates are located in `swiftgen-templates/viper/` and use Stencil syntax with the `{{moduleName}}` placeholder, which gets replaced with the actual module name during generation.

### Available Templates

- **interactor.stencil** - Generates the Interactor class
- **presenter.stencil** - Generates the Presenter class
- **protocols.stencil** - Generates all protocol definitions
- **viewcontroller.stencil** - Generates the ViewController class (UIHostingController for SwiftUI)
- **viewfactory.stencil** - Generates the ViewFactory enum
- **wireframe.stencil** - Generates the Wireframe class
- **viewlayout.stencil** - Generates the SwiftUI ViewLayout struct in PolkadotUI package (with preview)
- **viewmodel.stencil** - Generates the @Observable ViewModel class in PolkadotUI package

## Architecture

The generated modules follow the VIPER architecture pattern with SwiftUI:

- **ViewLayout**: SwiftUI view (public) in PolkadotUI package using `@State` to hold the ViewModel, includes a `#Preview`
- **ViewModel**: `@Observable` class (public) in PolkadotUI package that conforms to a protocol, used for state management
- **ViewController**: `UIHostingController` that wraps the SwiftUI view and updates the ViewModel
- **Presenter**: Updates the ViewModel through the ViewController, which directly modifies `rootView.viewModel`

This pattern follows the approach used in the Deposit module. ViewLayout and ViewModel are generated in the PolkadotUI package to allow reuse across the app.

## Customization

You can customize the templates by editing the `.stencil` files in this directory. The templates follow the project's existing VIPER module patterns with SwiftUI.

## Requirements

- Bash shell
- Standard Unix utilities (sed, mkdir, etc.)

Note: SwiftGen itself is not required for VIPER module generation, as the script uses simple template substitution. However, SwiftGen can be used for other code generation tasks (see `swiftgen.yml` in the project root).

