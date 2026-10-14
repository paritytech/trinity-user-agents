---
name: implementer
description: Implement changes following an existing PLAN.md. Validates scope, writes code, and self-reviews before handoff.
user_invocable: true
---

# Implementer Skill

## Preconditions

1. `.claude/PLAN.md` must exist — if not, tell the user to run `/architect` first
2. Read the full PLAN.md before writing any code

## Procedure

### Step 1: Read and Validate Plan

Read `.claude/PLAN.md`. Extract:
- `files_touched` — these are the ONLY files you may create/modify
- `seams_used` — integration points to be careful with
- `must_not_touch` — files you must NOT modify
- `out_of_scope` — features/changes explicitly excluded

If the plan seems incomplete or contradictory, ask the user before proceeding.

### Step 2: Load Relevant Code Docs

Based on the task, load code docs from `.claude/docs/code/` as needed:

| If implementing...          | Load                          |
|-----------------------------|-------------------------------|
| Error handling              | code/error-handling.md        |
| Async/concurrent code       | code/concurrency.md           |
| UI/layout                   | code/ui-uikit.md              |
| Naming decisions            | code/naming-and-hygiene.md    |
| Service wiring              | code/di-and-services.md       |
| CoreData/storage            | code/data-persistence.md      |
| SDK types/conversions       | code/project-types.md         |
| Navigation                  | code/navigation.md            |
| Tests                       | code/testing.md               |
| Design system/theming       | code/design-system.md         |

### Step 3: Implement

Follow the plan step by step. For each step:

1. Read existing code before modifying
2. Write the changes
3. Update peer files:
   - Protocols.swift when adding inter-layer methods
   - ViewFactory when changing view construction
   - Wireframe when adding navigation
   - Tests for new logic
   - Localization for new user-visible strings
4. Mark the step complete

### Peer Files

These files are implicitly in scope when their counterpart is in `files_touched`:

| If touching...              | Also update...                    |
|-----------------------------|-----------------------------------|
| Presenter method            | Protocols.swift                   |
| Interactor method           | Protocols.swift + Presenter       |
| ViewLayout                  | ViewController + ViewFactory      |
| Wireframe navigation        | Protocols.swift                   |
| New user-visible string     | Localization files                |
| New public type             | Tests (if test target exists)     |

### Step 4: Self-Review

Before declaring done, review your own changes against:

1. **Scope check** — did you touch only `files_touched` + peer files?
2. **`must_not_touch` check** — did you accidentally modify excluded files?
3. **`out_of_scope` check** — did you implement anything explicitly excluded?
4. **Code hygiene**:
   - No force unwraps (guard/throw instead)
   - No dead code introduced
   - Methods under 50 lines
   - Files under 400 lines
   - Lines under 120 characters
   - All user-visible strings localized
   - Private methods in extensions
5. **Reuse check** — did you write something that already exists in SubstrateSdk, StructuredConcurrency, FoundationExt, or PolkadotUI?
6. **Type safety** — using typed constants (HttpMethod, etc.), Decodable conformance, ScaleEncodable where appropriate?

### Step 5: Build Validation

If possible, validate the build:
```bash
xcodebuild -project polkadot-app.xcodeproj -scheme polkadot-app -configuration Debug build
```

Report build errors and fix them before returning control.

### Step 6: Rule Extraction

Check if the implementation revealed any patterns or decisions that should be documented:
- Did you discover a non-obvious constraint?
- Did the user correct your approach?
- Did you find a pattern that should be reused?

If so, suggest saving to memory or updating docs.

## Important Notes

- Stay within the plan's scope — no "improvements" beyond what was planned
- If you discover the plan needs changes, ask the user rather than deviating
- Prefer editing existing files over creating new ones
- Use the VIPER generator for new modules: `./generate-viper-module.sh ModuleName`
- Check xcstrings-crud availability for .xcstrings changes
- Check figma MCP for layout implementation from mockups
