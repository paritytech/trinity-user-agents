# Auditing an Existing Suite

Goal: fewer, stronger, deterministic tests. Audit one feature folder or package at a time
so the diff stays reviewable.

## Procedure

1. **Scope.** Pick a folder (`polkadot-appTests/<Feature>/` or `Packages/<Pkg>/Tests`).
   Record the baseline: test count (`rg -c '@Test' <dir>`) and suite runtime from a full run
   of that folder.
2. **Map tests to production units.** For each suite, name the production unit and behavior it
   protects. Triage the unit with `risk-triage.md`.
3. **Give each test a verdict:**

| Verdict | When |
|---------|------|
| **keep** | Tier 1/2 behavior, deterministic, asserts outcome through a public seam |
| **parameterize** | Several tests differ only in input/expected (enum-case round-trips, formatter cases) |
| **merge** | Several tests set up the same scenario and assert different fields of one outcome; assert the whole value once instead |
| **move-to-extracted-type** | Test inspects a humble object (labels, colors, fonts, hidden flags, view hierarchy, wireframe pushes) but the rule is real; extract the rule (`humble-objects.md`) and retarget the test |
| **rewrite-deterministic** | Valuable test that sleeps, polls with a quiet budget, uses a short timeout, or reads real time (`time-and-async.md`) |
| **delete** | Tier 3 unit (field copy mapper, pass-through, `init` wiring, Codable round-trip of hand-written code with no spec pin); duplicates coverage of a stronger test; asserts implementation (call order on internal mocks) rather than outcome |

4. **Guard against losing signal.** Before deleting, check whether the behavior is a
   regression test for a past bug (`git log -S '<test name>'`, linked issue). Those stay.
   For a deleted Tier 2 test, name the remaining test that still covers its path.
5. **Apply and run.** Run the folder with `-test-iterations 20` for anything async.
6. **Report.**

## Report format

```markdown
## Test audit: <folder>

Baseline: 84 tests, 41 s. After: 37 tests, 9 s.

| Suite / test | Unit | Tier | Verdict | Reason |
|--------------|------|------|---------|--------|
| VoucherMapperTests (5) | VoucherMapper | 3 (rule in `.inRecycler`) | parameterize to 1 | case round-trips; only associated value encoding is a rule |
| ChatTransferMessageViewTests (6) | view | humble | move-to-extracted-type (2), delete (4) | typography ladder and partial-claim are rules; colors/fonts are rendering |
| AppScannerHostBridgeTests.dismissal | bridge | 2 | rewrite-deterministic | 200 ms sleeps |

Follow-ups (production changes needed for testability):
- Inject clock into `SPAViewController` minimum loading delay (extract `MinimumDurationGate`).
```

## Common repo patterns to look for

- Per-enum-case Core Data mapper round-trips (`CoreDataMapperTests+*`): parameterize; delete
  pure field copies.
- PolkadotUI view tests reading `label.text` / `font` / `textColor`: extract view model.
- Local `waitUntil` helpers with fixed polling budgets that return silently (e.g. 200 x 5 ms in
  `RustRuntimeBridgeTests`): replace with event waits or one shared helper that fails.
- Wait helpers with short default timeouts (`MockLogger.waitForError`, 5 s): raise to a
  hang-guard budget.
- `Task.sleep` after `cancel()` "to let cleanup run": await the task value or a recorder event.
- Remaining XCTest suites: migrate only when the suite is kept.
