---
name: architect
description: Design implementation plan for non-trivial changes. Loads relevant architecture docs, asks clarifying questions, and produces a PLAN.md.
user_invocable: true
---

# Architect Skill

## When to Trigger

- Non-trivial changes (>1 VIPER layer, >1 package, or new pattern)
- Chat, products, transactions, coinage, or statement-store work
- Cross-feature refactors
- Any task where the user requests a plan

## Procedure

### Step 1: Load Documentation Index

Read `.claude/docs/README.md` to get the routing table.

### Step 2: Explore Relevant Code

Use the Explore agent to understand the current state of the codebase in the areas that will be touched. Include:
- Existing patterns in the target modules/packages
- Related code that might be affected
- Current implementations to build upon

### Step 3: Load Relevant Docs

Based on the routing table, load the architecture and code docs relevant to the task. At minimum:
- The architecture doc for the primary domain (chat, transactions, etc.)
- `architecture/maintainability.md` (always relevant)
- Any code docs for patterns that will be used

### Step 4: Clarification Phase (MANDATORY)

Before writing a plan, ask the user clarifying questions. Cover:

1. **Requirements ambiguity** — anything unclear about what should be built?
2. **Corner cases** — edge cases the user has considered?
3. **Error paths** — how should failures be handled?
4. **Gaps** — anything missing from the request?
5. **Doc contradictions** — do docs conflict with the requested approach?
6. **Design options** — are there trade-offs to discuss?
7. **Test strategy** — what level of testing is expected?

Do NOT skip this step. Ask focused questions, not a laundry list.

### Step 5: Write PLAN.md

Write the plan to `.claude/PLAN.md` with this structure:

```markdown
---
task: <short description>
date: <YYYY-MM-DD>
status: draft
files_touched:
  - path/to/file1.swift
  - path/to/file2.swift
seams_used:
  - <seam from docs>
must_not_touch:
  - <files/areas explicitly excluded>
out_of_scope:
  - <things that are NOT part of this task>
---

## Goal

<1-2 sentences: what we're building and why>

## Approach

<High-level design decision and rationale>

## Steps

### 1. <Step title>
- What to do
- Which files to create/modify
- Key implementation details

### 2. <Step title>
...

## North-Star Alignment

<How this change aligns with or progresses toward the project's north-star architecture>

## Risks

- <Risk 1 and mitigation>
- <Risk 2 and mitigation>

## Verification

- [ ] <How to verify step 1>
- [ ] <How to verify step 2>
- [ ] Build succeeds
- [ ] Tests pass
```

### Step 6: Rule Extraction

After the plan is finalized, check if the user's corrections or decisions during clarification revealed any non-obvious rules that should be saved to memory or docs. If so, note them for the user to confirm.

### Step 7: Exit Plan Mode

Call ExitPlanMode when the plan is complete and approved.

## Important Notes

- The plan is a CONTRACT — the implementer will follow it
- `files_touched` must be exhaustive — the implementer uses it as scope
- `must_not_touch` prevents scope creep
- `out_of_scope` documents what was explicitly deferred
- Keep the plan actionable — implementation details, not just high-level goals
