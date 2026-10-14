---
name: commit-message
description: Generate a well-structured commit message from staged changes.
user_invocable: true
---

# Commit Message Skill

## Procedure

1. Run `git diff --cached` to see staged changes
2. Run `git log --oneline -10` to see recent commit style
3. Analyze the nature of changes:
   - **add** — wholly new feature or file
   - **update** — enhancement to existing feature
   - **fix** — bug fix
   - **refactor** — code restructuring without behavior change
   - **chore** — build, CI, dependency updates
4. Draft a concise commit message (1-2 sentences) focusing on "why" not "what"
5. Match the repository's existing commit message style

## Format

```
<type>: <short description>

<optional body explaining why, not what>
```

## Rules

- Keep subject line under 72 characters
- Use imperative mood ("add", "fix", "update", not "added", "fixed")
- Focus on the motivation, not the mechanics
- Don't list every file changed
- Reference ticket numbers if present in the branch name (e.g., PANS-1234)
