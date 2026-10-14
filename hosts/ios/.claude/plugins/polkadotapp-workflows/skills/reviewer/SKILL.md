---
name: reviewer
description: Review a PR or diff against architecture and code checklists. Produces a verdict with blocking/major/minor findings.
user_invocable: true
---

# Reviewer Skill

## Procedure

### Step 1: Get the Diff

Determine what to review:
- If a PR number is provided: `gh pr diff {number} --repo paritytech/polkadot-app-ios-v2`
- If reviewing local changes: `git diff` or `git diff develop...HEAD`
- Read the PR description if available: `gh pr view {number} --repo paritytech/polkadot-app-ios-v2`

### Step 2: Read PLAN.md (if present)

If `.claude/PLAN.md` exists and relates to this PR:
- Read it fully
- Note `files_touched`, `must_not_touch`, `out_of_scope`
- Will check plan-vs-implementation alignment in Step 5

### Step 3: Load Checklists

Load the review checklists as the primary review source:
- `.claude/docs/review/architecture-checklist.md` — for structural/design review
- `.claude/docs/review/code-checklist.md` — for file-by-file code review

### Step 4: Walk the Diff

For each changed file:

1. Read the full file (not just the diff) to understand context
2. Apply relevant checklist items
3. Note any violations with severity:
   - **Blocking** — must fix before merge (correctness, security, data loss risk)
   - **Major** — should fix before merge (architecture violation, maintainability)
   - **Minor** — nice to fix (style, naming, minor improvement)

Group findings by theme, not by file.

### Step 5: Plan-vs-Implementation Diff (if PLAN.md exists)

Check:
- [ ] All `files_touched` in the plan were actually modified
- [ ] No files in `must_not_touch` were modified
- [ ] Nothing from `out_of_scope` was implemented
- [ ] The approach matches the plan's design decisions
- [ ] Verification criteria from the plan are addressed

### Step 6: Doc-Update Proposals

Check if the changes reveal:
- **Code-level gaps** — patterns used that aren't in `code/` docs
- **Architecture-level gaps** — new architectural decisions not in `architecture/` docs
- **Checklist gaps** — rules that should be added to review checklists

Propose specific doc updates if any gaps found.

### Step 7: Write Verdict

Format the review output:

```markdown
## Review: {PR title or description}

**Verdict:** X blocking / Y major / Z minor

### Blocking
- **[severity]** `file:line` — Description. **Fix:** concrete suggestion. *Ref: checklist-section*

### Major
- ...

### Minor
- ...

### Plan Alignment (if applicable)
- Matches plan: yes/no
- Deviations: ...

### Doc Updates Proposed
- ...
```

## Inline Comment Style

When posting GitHub comments:
- Tag severity: `[blocking]`, `[major]`, `[minor]`
- Quote `file:line`
- Cite the checklist rule
- Suggest a concrete fix
- 1-2 paragraphs max, no fluff

```
[major] `SomeInteractor.swift:42`

Force unwrap on optional chain result. This will crash if the chain is temporarily unavailable.

**Fix:** `guard let result = chainResult else { throw ChainError.unavailable }`

*Ref: code-checklist > Error Handling*
```

## Summary Body (for PR comment)

- Lead with overall sentiment (approve / request changes)
- List cross-cutting themes (not per-file noise)
- Process notes only if relevant (e.g., "plan says X but implementation does Y")
- Keep it actionable

## Important Notes

- The reviewer does NOT extract rules — only architect and implementer do
- If no PLAN.md exists, review against checklists only
- When in doubt about a rule, cite the specific doc section
- Focus on what matters: correctness > architecture > style
