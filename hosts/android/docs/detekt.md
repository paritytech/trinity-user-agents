# Detekt Code Quality

Static code analysis for Kotlin using **Detekt v1.23.8**.

## Quick Start

```bash
# Run analysis (auto-corrects fixable issues)
./gradlew detekt --continue
```

## Pre-commit Hook

Install the hook to run detekt automatically before each commit:

```bash
./developer-tools/setup.sh
```

The hook will:
1. Run detekt with auto-correct
2. Stage any auto-fixed files
3. Verify all issues are resolved
4. Block commit only if manual fixes are needed

Bypass if needed: `git commit --no-verify`

## Reports

HTML reports are generated at: `<module>/build/reports/detekt/detekt.html`

## Configuration

- **Config file**: `config/detekt/config.yml`
- **Formatting rules**: Enabled with auto-correct
- **Project rules**: `config/detekt/rules`, rule set `polkadotapp`. `HardcodedComposeText` fails the build when a composable passes a string literal as a named `text`, `title`, `subtitle`, `label`, `placeholder` or `headerText` argument; move the text to `strings.xml`. It only sees a string literal passed directly as a named argument, so `Text("Hello")`, strings chosen in `if`/`when` expressions, concatenated strings and strings held in variables are not detected. A `label` passed to one of the Compose animation APIs listed in `ANIMATION_CALLS` is a tooling name and is not reported; when a Compose update adds another animation API with a `label`, add its name to that list. Its tests run with `./gradlew :config:detekt:rules:test`.

## Suppressing Violations

Use only when truly justified:
```kotlin
@Suppress("MagicNumber")
fun calculateDiscount(): Double = 0.15
```

## Links

- [Detekt Docs](https://detekt.dev/)
- [Rules Reference](https://detekt.dev/docs/rules/formatting/)
