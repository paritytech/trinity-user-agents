# Design System

## Overview

The design system is tokenized and distributed across two packages:
- **PolkadotUI** (`Packages/PolkadotUI/`) — app-level design system components, typography, shared views
- **polkadot-app-design-system-ios** (external) — tokenized themes, typography definitions, radii

## Architecture

```
polkadot-app-design-system-ios (tokens/themes)
        ↓
PolkadotUI (components + module views)
        ↓
polkadot-app/Modules/ (feature screens)
```

### ThemeManager
- Dark mode enforced (`UIUserInterfaceStyle: Dark` in Info.plist)
- Set up in `SceneDelegate` on app launch
- "setup" naming for initialization methods

### TypographyManager
- Typography system initialized alongside theme
- Font management through the design system

## Usage Rules

1. **Use design system tokens** — colors, spacing, typography from the theme, not hardcoded values
2. **Dark mode only** — do not add light mode support
3. **Components go in PolkadotUI** — reusable across modules
4. **Module-specific views** go in `PolkadotUI/Sources/Modules/`
5. **Don't create unnecessary abstractions** — if UIFont conversion works directly, don't create a Font extension wrapper
6. **SwiftUI color shorthand** — use `.fgPrimary` when type context allows, not `Color(.fgPrimary)` or `Color.fgPrimary`
7. **Discuss missing tokens with design team** — when a spacing/layout value isn't in the DS, discuss with designers rather than silently adding `TODO: Should be in DS`
8. **Add transitions for state changes** — when SwiftUI views switch visual states (backgrounds, images), add opacity/crossfade transitions

## Liquid Glass (iOS 26)

- Liquid Glass design support is implemented
- Platform-specific UI adaptations for newer iOS versions

## From PR Reviews

- "Why do we need extension Font if we are converting from UIFont?" — unnecessary wrapper
- "Why do we need both `ThemeMode` and `ThemeSelection`? How they are different?" — every abstraction needs distinct purpose
- "'setup' is more applicable here since it is used for theme setup on app launch" — naming reflects usage context

## MCP Tool

When available, use the `figma` MCP server to understand layers and styles from Figma mockups for layout implementation.
