# Risk Triage: What Deserves a Test

A unit test pays for itself only when the code it protects is likely to break *and* a break
would hurt. Score both, then test in proportion.

## Step 1: Score each unit

**Impact (I)**, what a defect costs:

| I | Examples in this app |
|---|----------------------|
| 3 | Loss or misdirection of funds, key material, signing payloads, SCALE/wire encoding pinned by a spec, persisted state that cannot be recovered (Coinage ledger, durable transactions, device sync), auth/SSO decisions, permission and allowance checks |
| 2 | Wrong but recoverable user-visible state: balance shown, message status, retry/backoff giving up too early or never, stale cache served, notification not delivered |
| 1 | Cosmetic or self-healing: ordering of a list that re-sorts, a label, a placeholder |

**Likelihood (L)**, how easily it breaks:

| L | Signals |
|---|---------|
| 3 | Branching on state machines, arithmetic on amounts/fees/indices, concurrency (actors, reentrancy, cancellation, streams), time (TTL, retries, timeouts, expiry), parsing untrusted input, code that changed often (`git log --follow` churn) or had bugs |
| 2 | A few conditionals, input validation, combining two sources |
| 1 | Straight-line code: field copies, enum-to-enum maps without logic, `init` wiring, pass-through calls, layout |

## Step 2: Tier = I x L

| Score | Tier | Treatment |
|-------|------|-----------|
| 6 to 9 | **Tier 1** | Full behavioral coverage: happy path, every outcome-changing failure, boundaries, cancellation/retry/time transitions. Consider a property-style parameterized test over generated inputs for arithmetic and codecs. |
| 3 to 4 | **Tier 2** | Happy path plus the most likely failure. Often covered indirectly by a Tier 1 test that runs through it; then add nothing. |
| 1 to 2 | **Tier 3** | No dedicated test. Covered implicitly by tests of the caller, by the compiler, or by manual QA. |

Overrides that always force Tier 1 regardless of score:
- Pinned compatibility with a spec or another platform (test vectors, wire indices, byte layouts).
- A bug fix: the regression test that reproduces it stays.

## Step 3: Map to the code's quadrant

(Khorikov, *Unit Testing Principles*, ch. 7.) Complexity/domain significance on one axis,
number of collaborators on the other:

| | Few collaborators | Many collaborators |
|---|---|---|
| **High complexity** | **Domain & algorithms**: unit test heavily (Tier 1 lives here) | **Overcomplicated**: split before testing; push logic down, keep a thin coordinator |
| **Low complexity** | **Trivial**: do not test | **Controllers / coordinators**: few integration-style tests through real types, doubles only at process boundaries |

In VIPER terms:

| Layer | Usual quadrant | Default |
|-------|----------------|---------|
| Interactor / Service with rules | Domain or Overcomplicated | Extract rules into pure types; test them. Test the service through its protocol with doubles at the network/chain/clock seam. |
| Presenter | Controller | Test only real decisions (state transitions, error to message choice). Formatting moves to a `ViewModelFactory`; test it only if it has branches. |
| ViewModelFactory / formatter | Domain if branching, else Trivial | Parameterized table test when branching exists. |
| ViewController / ViewLayout / SwiftUI view / cell | Humble | No unit test. |
| Wireframe / ViewFactory | Humble | No unit test. |
| Core Data mapper | Trivial unless it encodes a rule | Test only rules (defaults for missing data, migrations, recovered-state semantics). One round-trip per entity is enough when the mapping is lossy or custom-encoded; none when it is a field copy. |
| Codable / SCALE types | Trivial unless spec-pinned | Only pinned vectors (see testing.md, Coding Tests). |

## Coverage target: 70 to 80% of risky code, not of the app

- Measure line coverage **only over files that contain Tier 1/2 units**, not over the whole
  target. A global percentage rewards testing trivial code and humble objects.
- Use coverage to find untested risky branches, never as a goal to reach by adding tests.
- Mutation testing (Muter) on Tier 1 packages is the better signal of test strength than
  coverage. Run it on demand on one package (e.g. `Packages/Coinage`), never in PR CI.

Per-file report from an xcresult:

```bash
xcrun xccov view --report --files-for-target polkadot-appTests.xctest  Test.xcresult  # discover names
xcrun xccov view --report --json Test.xcresult \
  | jq -r '.targets[].files[] | select(.path | test("Coinage|DurableTransactions|Funding")) | "\(.lineCoverage) \(.path)"'
```

## Triage output format

```
| Unit | I | L | Tier | Risk driver | Behaviors | Seam / doubles |
|------|---|---|------|-------------|-----------|----------------|
| ClaimCoinsService.claim | 3 | 3 | 1 | funds, retries over chain time | claims once; retries on transient error; gives up after deadline; idempotent on resume | chain client, ManualClock |
| VoucherMapper | 1 | 1 | 3 | field copy | none | - |
| ChatTransferMessageView | 1 | 2 | humble | amount typography ladder is a rule | extract `TransferAmountStyle.for(amount:)`, test that | - |
```
