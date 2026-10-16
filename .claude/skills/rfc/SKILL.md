---
name: rfc
description: Draft a short RFC for this repo. Use when the user wants to create, draft, or write an RFC.
argument-hint: [topic or brief description]
context: fork
---

# RFC

Write a 1-2 page RFC. It exists to get agreement on an approach, not to specify it: details are the implementer's call
and need no prior approval here.

## Mechanics

- Title the pull request `docs(rfc): <title>`, per the [`semver-pr-title`](../semver-pr-title/SKILL.md) skill. `RFC: <Title>` is the form for the tracking issue.
- File `docs/rfcs/<kebab-title>.md` from [docs/rfcs/0001-template.md](../../../docs/rfcs/0001-template.md). **Do not
  number it** and **do not touch `_index.md`** — `number-rfc.yml` assigns the number on merge to `main` and rebuilds the
  index from the files on disk.
- Keep the H1 as `# RFC — Title`, em dash included; the numbering step rewrites that exact form to inject the number.
- Set `status: draft` in the frontmatter. Omitting it makes CI index the RFC as `accepted`.
- `check-rfc.yml` reads the document: a new RFC needs `title` and `owner` in its frontmatter, a `## Summary`, a
  `## Motivation`, and a section covering the approach, and no draft may keep unedited template text or a `TODO`. Rust
  changes are not required in the same PR.

## Writing it

- Lead with the problem. If it isn't concrete, the RFC isn't ready.
- Describe what changes and why, not signatures, thresholds or edge cases.
- Cut every sentence that would not change a reader's mind.
- Prefer a stated assumption to a blocking question. Ask the author only when the answer changes the approach.
- Omit any section you would otherwise fill for the template's sake.

## Style

Rules for writing and reviewing RFCs and design documents in this repository. An RFC is a strict proposal, not an essay:
it states what the system does, and nothing else.

### Cut

- Selling text and justification. State the rule; drop the reasoning and the "why this is good".
- Enumerations of examples. One case or none; one when the enumeration is the only illustration of a rule.
- Em-dash appositions and dash-appended lists. Use plain sentences, a colon, or "because" and "so".
- Bold-lead paragraphs that act as mini-sections. Fold them into the surrounding text or make a real heading.
- Self-references: "this RFC", "this section", "below", "above", "stated under X".
- Transition narrative: "previously X, now Y", "matters more now that", "returns what it returned before". Describe the
  resulting state.
- Sections that restate the summary or another section.
- Recommended defaults marked "unvalidated", letter variables standing in for them, and any open question that exists
  only to hold them. A concrete value is implementation policy unless it is normative.
- Metaphors that introduce their own vocabulary. Use the defined term.

### Shape

- Requirements are constraints any acceptable design must satisfy, not a summary of the chosen design. Four or five
  items, each one bold word and one sentence.
- One bullet is one rule, one or two sentences. Long lists become one-liners without bold leads.
- The design roadmap is a bullet list introduced by "The design has N parts:", with nothing after it.
- Tables have a one-line caption above them, and rows are phrased in parallel.
- Rust blocks are complete: the full `pub trait X: Send + Sync { ... }` with default bodies, in the shape of
  `rust/crates/truapi/src/api/*.rs`. Every field and variant carries a doc comment.
- A concept shared by several RFCs is defined in one and linked from the others. A dependent RFC states only what it
  adds.
- A term is defined once. Before cutting or moving a definition, find every use of the term and put the definition at
  the best of them: the earliest use, or a Definitions section if the document has one. All other uses stay bare.
- An inline list is a set of examples unless it is declared exhaustive: write "may be" or "such as". An exhaustive set
  goes in a table or an enum.

### Process

- Read the whole document before editing, then trim section by section in reading order.
- Earlier text wins. An approved paragraph is normative for everything below it; reconcile downstream text to match.
- After each edit, search the rest of the document for terms the edit invalidated and fix them in the same pass.
- After each pass verify: no dangling references or undefined terms.
- Deleting a whole section is the author's call, not the reviewer's.

## Before handing it over

Re-read it as a reviewer with ten minutes. Cut what you would skim. Then tell the author what you cut and what you
assumed.
