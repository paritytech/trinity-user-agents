---
name: rfc-issue
description: Write the tracking issue for an RFC — a short Description / Motivation / Requirements / Tasks body labeled `area:rfc`, written either from an RFC document already in review or from notes when no RFC exists yet. Use for "open a tracking issue for this RFC", "create issues for the open RFCs", or when an idea needs an RFC that nobody has drafted.
argument-hint: [RFC PR number, RFC path, or a description of the idea]
---

# RFC tracking issue

One issue per RFC. The document carries the specification and its review; the issue
carries the work — landing the document, then implementing it in rust-core and in each
host. They stay separate because a merged RFC is not a shipped RFC, and the PR closes
long before the hosts catch up.

## Shape

**Title** — `RFC: <Title>`, with the RFC number dropped. `RFC 0025: Credential-endpoint
remote permission` becomes `RFC: Credential-endpoint remote permission`. The number
belongs to the document: it changes when the document is renumbered and it collides
between RFCs in flight.

**Label** — `area:rfc`.

**Body** — four sections, nothing else:

```markdown
**Source PR:** #<pr> · `docs/rfcs/<file>.md` · @<author>

## Description

One or two sentences: what the protocol does once this lands.

## Motivation

One or two sentences: the gap that exists without it.

## Requirements

- Three to five one-line bullets: the load-bearing constraints an implementer
  cannot get wrong.

## Tasks

- [ ] RFC document body
- [ ] Implementation — rust-core
- [ ] Implementation — hosts
  - [ ] dotli
  - [ ] Desktop
  - [ ] iOS
  - [ ] Android
  - [ ] host-cli
```

## Writing each section

**Source line.** `#<pr>` is the whole linking mechanism — GitHub autolinks it and writes
a cross-reference into the PR's timeline, so the two connect in both directions with no
comment on the PR.

**Description.** What the system *does* once this lands: "the manifest defines four
executable types", never "this PR adds a fourth type". The issue outlives the PR, and
after a squash-merge the transition wording is noise.

**Motivation.** The gap, stated concretely enough that someone can disagree with it.
"Products cannot tell which host is running them" beats "improves observability".

**Requirements.** The handful of things an implementer cannot get wrong — a wire-level
constraint, an ordering rule, a security property, a normative MUST. Not a précis of
Detailed Design. If a bullet needs a second line to survive, it belongs in the document,
not here.

**Tasks.** Fixed. Five host boxes stay in every issue even where a host plainly will not
implement this one: a missing box reads as done, an unchecked box reads as outstanding,
and only one of those is true.

An open problem with the document itself — a colliding RFC number, a `00XX` placeholder
filename — goes in one italic `_Note:_` line after Tasks. Nothing else is appended.

## When no RFC PR exists

The issue often comes first: a gap surfaces in review of something else, or the user
describes a change nobody has drafted. Write the same four sections, sourced from the
conversation and the code rather than from a document, and change two things:

- Replace the source line with where the idea came from — `**Raised in:** #444
  (review)`, `**Design doc:** docs/design/<file>.md`, or `**Source:** discussion with
  @handle`. Never invent a `docs/rfcs/` path for a file that does not exist.
- Say so, so nobody looks for the document: `_Note: no RFC drafted yet._`

Requirements are the part that goes thin here, and that is honest — write the
constraints that are actually settled and leave the rest to the RFC. A requirement
invented to fill the section will be read as a decision someone made.

Then hand off: the `rfc` skill drafts the document, and this issue is the notes it
interviews against. The `RFC document body` box is what tracks that gap.

## Before opening anything

Sixteen issues land in everyone's notifications at once. Draft the bodies, show them,
and get a yes before the first `gh issue create`.

Check for an existing issue first. Two PRs against one document are still one document:
an RFC amending `product-manifest.md` gets its own issue, but a second PR against the
same amendment does not.

```bash
gh issue list --label rfc --state all --limit 100
gh issue create --title "RFC: <Title>" --body-file <path> --label rfc
```

## Related

`.claude/skills/rfc/SKILL.md` writes the document itself. That skill ends at the
document; this one covers the issue on either side of it.
