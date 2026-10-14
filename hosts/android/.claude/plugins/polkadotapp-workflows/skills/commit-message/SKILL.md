---
name: commit-message
description: Generate a well-formatted Git commit message from a description of changes
version: 1.0.0
---

# Commit Message Writer

Generate a well-formatted Git commit message from a description of changes.

## Format Rules

1. **Title**: single line, 72 characters max, imperative mood (e.g. "Add", "Fix", "Update" — not "Added" or "Adding")
2. **Blank line** between title and description
3. **Description**: explain *what* and *why*, each line wrapped at 72 characters
4. **Blank line** after description
5. **Issue reference**: `Issue: <TICKET>` on its own line at the end

```
<title — max 72 chars, single line>

<description line 1 — max 72 chars>
<description line 2 — max 72 chars>
...

Issue: XXXX-0000
```

## Behavior

- Ask the user for:
  - A description of what changed and why (if not provided)
  - The ticket number (if not provided)
- Never exceed 72 characters on any line
- Never put the ticket on the same line as the description
- Never skip the blank line between title and description
- Never skip the blank line between description and issue reference
- If the user provides a rough message, reformat it to comply with the rules above
- If the title would exceed 72 chars, shorten it and move detail into the description

## Examples

**Good:**

```
Fix null pointer crash on empty cart checkout

The cart total calculation assumed at least one item was present.
Added a guard clause to return early when the item list is empty,
preventing the crash on first-time users with no cart history.

Issue: PANS-1861
```

**Good (short change):**

```
Bump minimum iOS deployment target to 16.0

Issue: PANS-2034
```

**Bad (do not produce):**

```
fixed the bug where it crashes (title not imperative, vague)
description text here Issue: PANS-1861  ← no blank line, ticket inline
```

## Invocation

When a user says something like:
- "Write a commit message for…"
- "Generate a commit for…"
- "Help me commit…"

…apply this skill. If the ticket number is missing, ask for it before producing the final message.
