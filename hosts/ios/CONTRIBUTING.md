# Contributing

Thanks for your interest in improving the Polkadot iOS app. Issues and pull
requests are welcome.

## Reporting issues

- Search [existing issues](https://github.com/paritytech/polkadot-ios-community/issues)
  before opening a new one.
- For bugs, include: device/OS version, app build configuration, reproduction
  steps, expected vs. actual behavior, and logs or screenshots where relevant.
- For feature requests, describe the problem you're trying to solve, not just a
  proposed solution.
- **Do not** report security vulnerabilities in public issues. Follow the
  reporting process described in
  [Parity's security policy](https://github.com/paritytech/.github/blob/main/SECURITY.md) instead.

## Development setup

Requirements: Xcode with the iOS 17.0+ SDK.

```bash
git clone https://github.com/paritytech/polkadot-ios-community.git
cd polkadot-ios-community
./Runscripts/setup-secrets.sh
open polkadot-app.xcodeproj
```

`Runscripts/setup-secrets.sh` scaffolds the gitignored config files from templates
and generates the build-time configuration. Swift Package Manager dependencies
resolve automatically on first build. See [CLAUDE.md](./CLAUDE.md) for the
architecture overview and [docs/PUBLISHING.md](./docs/PUBLISHING.md) for
build-time configuration and publishing.

## Pull requests

1. Fork the repository and create a topic branch from `develop`.
2. Keep commits small and atomic, with clear messages.
3. Follow the code style (see below) and add tests where it makes sense.
4. Make sure the project builds and the test suite passes locally.
5. Open the PR against `develop`, describe the change and link any related issue.

```bash
# Build
xcodebuild -project polkadot-app.xcodeproj -scheme polkadot-app -configuration Debug build

# Unit tests
xcodebuild test -project polkadot-app.xcodeproj -scheme polkadot-appTests \
  -destination 'platform=iOS Simulator,name=iPhone 16'
```

## Code style

- **Architecture:** every feature is a VIPER module. Scaffold new ones with
  `./generate-viper-module.sh ModuleName`.
- **Linting/formatting:** run `swiftlint lint` and `swiftformat .` before
  pushing. Configs live in [.swiftlint.yml](./.swiftlint.yml) and
  [.swiftformat](./.swiftformat). Keep functions focused (prefer under 50 lines)
  and lines under 120 characters.
- **Naming:** follow the VIPER suffix conventions (`ViewController`, `Presenter`,
  `Interactor`, `Wireframe`, `ViewFactory`, `Protocols`).

## Licensing of contributions

By contributing, you agree that your contributions are licensed under the
project's [GNU General Public License v3.0](./LICENSE). Don't add dependencies or
code under licenses incompatible with GPL-3.0.
