---
name: commit-message
description: Generate a well-structured commit message from staged changes.
user_invocable: true
---

# Commit Message

Write the commit message, and the pull request title, as a conventional commit.
The pull request title becomes the squashed commit on `main`, so the format and
type rules in the repository's `.claude/skills/semver-pr-title/SKILL.md` apply,
including when a change is breaking and needs `!`.

## Format

```
<type>(<scope>)!: <subject>

<body: why the change is needed, wrapped at 72 characters>
```

- `type` is one of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`,
  `ci`, `chore`, `style`, `revert`. Use `ios` as the scope, for example
  `fix(ios): ...`.
- `!` only when the change breaks something for testers or products, as the
  semver skill defines it.
- The subject is imperative, lower case and under 72 characters, with no full stop.
- The body says why, not which files changed. Omit it for an obvious change.
- Close a GitHub issue with `Closes #N` in the pull request description.
- Never add a `Co-Authored-By` trailer for an AI. The CLA check rejects it, and
  the pull request has to be recreated.

## Procedure

1. Read `git diff --cached`.
2. Pick the type from what the change does for a user of the app, not from how
   much code moved.
3. Write the subject and, if the reason is not obvious, the body.
