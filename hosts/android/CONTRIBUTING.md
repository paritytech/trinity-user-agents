# Contributing to Polkadot Android

Thanks for your interest in contributing! This project welcomes issues and pull
requests. This guide explains how to get set up, what we expect in a
contribution, and how changes get reviewed and merged.

## Ways to Contribute

- **Report a bug** — open a [GitHub issue](../../issues) describing what you
  expected, what happened, and how to reproduce it.
- **Request a feature** — open an issue describing the use case and motivation
  before investing in a PR, so we can align on direction.
- **Submit a fix or improvement** — open a pull request (see below).

For **security vulnerabilities, do not open a public issue.** Follow the
reporting process described in
[Parity's security policy](https://github.com/paritytech/.github/blob/main/SECURITY.md).

## Getting Set Up

This is an Android application built with Gradle and Kotlin.

Requirements:

- JDK 21
- Android Studio (latest stable) or the Android SDK + command-line tools
- A `local.properties` pointing at your Android SDK (`sdk.dir=...`)

Clone and build:

```bash
git clone <your-fork-url>
cd polkadot-android-community
./gradlew assembleDebug
```

## Before You Open a Pull Request

Run the same checks CI runs locally:

```bash
# Static analysis (Detekt, with auto-correct for fixable issues)
./gradlew detekt --continue

# Unit tests
./gradlew testGpDebugUnitTest testGpReleaseUnitTest testDebugUnitTest testReleaseUnitTest
```

We recommend installing the pre-commit hook so Detekt runs automatically on each
commit:

```bash
./developer-tools/setup.sh
```

See [docs/detekt.md](docs/detekt.md) for details on the static-analysis setup.

## Coding Guidelines

- Follow the patterns already present in the surrounding code — match naming,
  structure, and idioms of the module you are touching.
- Keep changes focused. One logical change per pull request makes review faster.
- Extract user-facing strings to the appropriate `strings.xml` resources rather
  than hardcoding them.
- Prefer the project's design-system components over raw Material widgets.
- Add or update tests for any behavior you change.

## Commit Messages

We use clear, imperative commit messages:

1. **Title** — a single line, max 72 characters, imperative mood
   ("Add", "Fix", "Update" — not "Added" or "Fixing").
2. A **blank line**, then a description explaining *what* changed and *why*,
   wrapped at 72 characters.
3. If the change relates to a GitHub issue, add a trailing `Issue: #<number>`
   line (e.g. `Issue: #42`).

Example:

```
Fix null pointer crash on empty cart checkout

The cart total calculation assumed at least one item was present.
Added a guard clause to return early when the item list is empty.

Issue: #42
```

## Pull Request Process

1. Fork the repository and create a topic branch from `main`.
2. Make your change, keeping commits focused and tests passing.
3. Run Detekt and the unit tests locally (see above).
4. Push your branch and open a pull request against `main`. Describe the
   change, the motivation, and any testing you performed. Link related issues.
5. CI will run linting, tests, and a build. Please make sure these pass.
6. A maintainer will review your PR. Address review feedback by pushing
   additional commits to the same branch.
7. Once approved and green, a maintainer will merge it.

## License

This project is licensed under the **GNU General Public License v3.0** (see
[LICENSE](LICENSE)). By contributing, you agree that your contributions will be
licensed under the same terms.
