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

## Suppressing Violations

Use only when truly justified:
```kotlin
@Suppress("MagicNumber")
fun calculateDiscount(): Double = 0.15
```

## Links

- [Detekt Docs](https://detekt.dev/)
- [Rules Reference](https://detekt.dev/docs/rules/formatting/)
