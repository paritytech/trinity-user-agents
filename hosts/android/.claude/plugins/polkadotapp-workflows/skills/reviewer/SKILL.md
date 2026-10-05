---
name: reviewer
description: Audit a branch diff or GitHub PR against the PolkadotApp architecture and code rules. Cites the checklists as primary source of truth; reads `.claude/PLAN.md` (if present) to diff intent vs implementation. Produces a console comment list grouped by theme and tagged blocking/major/minor.
version: 1.0.0
---

# Reviewer

You audit a diff for violations of PolkadotApp architecture and code rules. **Do not look at existing PR comments** — review fresh from the diff alone. Output goes to the console; never write GitHub comments unless explicitly asked.

## Procedure

1. **Get the diff**:
   - User passes a PR number: `gh pr diff <N>` (and `gh pr view <N> --json files,title,body` for context).
   - Otherwise: `git diff master...HEAD` and `git log master..HEAD --oneline`.

2. **Read `.claude/PLAN.md` if present.** If a plan exists for the current work, parse the frontmatter — note `modules_touched`, `must_not_touch`, `files_touched`, `seams_used`, `out_of_scope`. You'll diff intent against the actual implementation in step 5.

3. **Load the matching checklist(s) — they are the primary citation source.** Route by what the diff touches:
   - Touches `feature/<X>/api/`, module wiring (`@Module`, `@Binds @IntoSet`, registries), new `.kt` files that define a contract/interface, anything in `chains/`, `database/`, or any new transaction/origin/extension class → load `.claude/docs/review/architecture-checklist.md`.
   - Touches a ViewModel, `@Composable`, `Mapper`, `Worker`, `Router`/`Navigator`, repository, DAO, or any rule-bearing concrete `.kt` file → load `.claude/docs/review/code-checklist.md`.
   - If both are touched (common), load both.
   - If you can't tell from paths alone, default to loading both.

   The checklist rules are severity-tagged and ready to cite. Quote the rule and the section directly; the architecture/code docs are **rationale**, not primary citation.

4. **Identify subsystems touched** from file paths, and load architecture/code docs only for *additional rationale* when the checklist alone doesn't cover the case:
   - `feature/chats/` or `ChatExtension`/`ChatBot` → `architecture/chat-extension.md`.
   - `feature/products/` or `HostCallHandlerGroup`/`HostApiSession`/`ProductTrUAPIHostBridge`/`HostApiInteractor`/`SpaHost` → `architecture/host-api-products.md`.
   - `feature/transactions/`, `feature/people/`, `feature/coinage/`, `TransactionOrigin`/`TransactionExtension` → `architecture/transactions.md`.
   - `feature/coinage/`, `ExternalPaymentService`, RFC-0006 → `architecture/coinage.md`.
   - `feature/statement-store/`, `CommunicationSession`, statements → `architecture/statement-store-communication.md`.
   - `tools/media-connection/`, `feature/calls/`, `DataTransport`/`PeerChannelSignaling` → `architecture/data-transport.md`.
   - `chains/` or `query`/`observe`/`callRuntimeApi`/`Scale.encode` → `architecture/chain-integration.md`.
   - Any ViewModel → `code/state-management.md` + `code/results-and-errors.md`.
   - Any `@Composable` → `code/ui-compose.md`.
   - Any Room entity/migration / SCALE conformance test → `code/database-and-scale.md`.
   - Any Dagger module / `App.kt` / `Service` → `code/di-and-lifecycle.md`.
   - Any `*Router.kt` / `*Navigator.kt` / new screen → `code/navigation-and-routers.md`.
   - Any `*Worker.kt` / `*StateMachine.kt` → `code/workers-and-background-sync.md`.
   - Any `markRegion` / `StalenessReport*` usage, or a new one-shot operation that submits an extrinsic or reads chain state on a user action → `code/diagnostics-and-stall-reporting.md`.
   - Any `*Test.kt` under `src/test/` (Mockito usage, helper naming, `runBlocking<Unit>`) → `code/testing.md`.

5. **Plan-vs-implementation diff** (only when PLAN.md is present):
   - Files in `must_not_touch` actually touched: **blocking**.
   - Items the plan listed in `out_of_scope` that were implemented anyway: flag as **major** scope creep.
   - Seams introduced not in `seams_used`: flag as **major** "new seam not in plan".
   - Files touched in the diff but **not** in `files_touched` AND **not** a peer (per `skills/implementer.md § Peer files`): flag as **major** "out of plan scope; was this approved?".
   - Peer-file edits (test siblings, DI modules wiring a touched class, `Real*` for a touched interface, `@Preview` for a touched composable, layer-mirror files, locally-scoped new strings): **do not flag**.
   - Items in `open_questions` that the implementation answers without surfacing: flag as **minor** "open question silently resolved".

6. **Walk the diff file by file.** For each violation:
   - **Quote** `file:line`.
   - **State the rule** in one short line and cite the checklist section: `(review/code-checklist.md § Result and errors)`. If a rule isn't in the checklists but is in an arch/code doc, cite both: `(review/code-checklist.md, rationale: code/results-and-errors.md § getOrThrow)`. If a rule is only in an arch/code doc, **propose adding it to the checklist** in the Doc-update proposals section.
   - **Suggest** a concrete fix in 1–2 sentences (or up to 3 lines of Kotlin).
   - **Tag severity**:
     - **blocking** — breaks an architectural invariant or RFC-0002 alignment.
     - **major** — clearly violates a documented code rule.
     - **minor** — naming, comments, redundancy, magic constants, file size.

7. **Group findings** by theme: Architecture / Modules / Plan-vs-impl / State / UI / Result handling / Naming / DI / Database+SCALE / Chat / Transactions / Coinage / Statement-store / Data-transport / Workers / PR hygiene.

8. **Doc-update proposals** (separate section, never blocking, never major — pure suggestions):
   - **Code-level**: a new helper / recurring shape / idiom shows up 2+ times in the diff that isn't documented in `code/*.md` — propose a section, name the file and suggested placement.
     - Example: "Three new VMs build state via a new `withRetry { … }` operator — propose adding to `code/state-management.md § Common operators`."
   - **Architecture-level** (flag only when the PR description **explicitly** says it's an architectural change — don't speculate from code alone): new module-layout, seam, or composition shape that isn't in `architecture/*.md` — propose extending the matching doc.
   - **Checklist gaps**: rules cited only from arch/code docs (not present in `review/*.md`) — propose adding to the matching checklist.

9. **End with a verdict**: blocking-count, major-count, minor-count; mergeable / needs major rework / blocked.

## Style

- Terse. Quote the rule and the line; don't restate what the code does.
- Skip noise: whitespace, formatter-level nits, anything not tied to a doc section.
- Collapse repeated violations across many lines into one comment with line list.
- Cite the checklist section first; rationale doc only if needed.

### Inline GitHub comments (when posting via `gh api .../reviews`)

An inline comment is already anchored to a `file:line`. The reader sees the surrounding code automatically. Optimize for terseness:

- **No path/class/method name that the comment is anchored to.** GitHub shows it. Don't say "in `RealFooUseCase.doStuff()` …" when the comment is *on* that line.
- **One short paragraph for the issue, one short paragraph (or a tiny code block) for the fix.** Two paragraphs max. If you need more, you're in the wrong format.
- **Concrete fix suggestions belong inline.** The author reads the comment + the surrounding code in one view; that's where actionable detail belongs. Don't push fixes up to the summary.
- **Cross-cutting refactors that span multiple inline comments DO go to the summary** — name the shape once there, then have inline comments reference it ("see summary for the launcher seam"). The split is: per-line fix → inline; one refactor that absorbs several findings → summary.
- **Don't name unrelated classes/files unless essential.** "Regresses behavior the deleted `RealXxxUseCase` got right" is fine when the contrast is the bug.
- **Code suggestions: minimal diff only.** A 2-3 line code block with the single substitution. No 10-line refactors.
- **Severity tag stays.** `**blocking**` / `**major**` / `**minor**` at the start of the body.

### Review summary body

The summary is for content the inline comments can't carry. It must NOT restate the inline findings — the author will read every inline; duplicating them in the summary doubles the reading work. Include only:

1. **Sentiment / posture** in one or two lines: "mergeable conditional on the X majors", "blocked on data-correctness", "solid; minors only".
2. **Cross-cutting refactors** that span 2+ inline comments. Name the shape once here; reference from inline.
3. **Process notes** not tied to one line: "link a tracking issue for the mock-payload follow-up", "this PR mixes two unrelated tasks".

Skip from the summary:
- Bulleted re-listings of each inline finding ("Headline points: — ... — ...").
- Per-issue fix suggestions (those are inline).
- Repeated doc § citations.

Console output (no GitHub) keeps the full theme-grouped format from "Output shape" — there the developer reads everything in one sitting and the cross-refs aid navigation.

## When you must NOT raise an issue

- Style preferences not in any doc.
- Speculation about future maintenance without an explicit rule.
- Anything you can't tie to a concrete doc section or quoted rule.
- **Anything ktlint already flags.** Trailing commas, import order/grouping/wildcards, multi-line-constructor whitespace, brace style, max-line-length, blank-line counts, space-around-operator, single-statement body formatting. The linter runs in CI and pre-commit — raising the same finding as a reviewer `minor` doubles the work for the author and dilutes signal. Reviewer is the layer above the linter: semantics, naming, layering, design. (`feedback_no_ktlint_comments` memory has the explicit list.)

## Reviewer does NOT extract rules from this session

Unlike `/architect` and `/implementer`, the reviewer skill does **not** propose or add rules to the docs based on the current invocation. Patterns the reviewer spots that aren't yet a documented rule go into the **`Doc-update proposals`** output section — they are *suggestions for the user*, never auto-added. The feedback loop for rule additions runs through `/architect` and `/implementer` only, where the user is iterating in the design/coding loop and can confirm rules interactively.

## Output shape

```
## Review: <PR title or branch>

### Plan-vs-implementation
- ✗ files outside `files_touched`: …
- ✓ scope respected for everything else.
(Skip this section entirely if no PLAN.md exists.)

### Architecture (N blocking, M major)
- **blocking** `file/path.kt:42` — <rule one-liner> (`review/architecture-checklist.md § <section>`)
  Fix: <…>

### State management (…)
…

### Doc-update proposals
- `code/state-management.md` — add a section on `withRetry { … }` operator: 3 new usages in this diff (FooViewModel:42, BarViewModel:88, BazViewModel:101).
- `review/code-checklist.md` — rule "no `getOrThrow` in Repository" currently only in `code/results-and-errors.md`; promote to checklist.

### Verdict
- 2 blocking, 5 major, 8 minor.
- Not mergeable until blocking items addressed. Recommend rework of <area>.
```
