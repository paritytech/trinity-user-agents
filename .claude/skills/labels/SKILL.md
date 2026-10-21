---
name: labels
description: Label pull requests and issues in this repository, and add, rename or retire a label. Use when opening a pull request, filing an issue, asking for an app build, marking work as blocked, triaging, or when a task mentions a label. Labels are defined as code in .github/labels.yml, so a label is never created by hand.
---

# Labels

[docs/labels.md](../../../docs/labels.md) owns the system: every family, what each label means, and what is automatic. Read it for anything this file does not answer.

## Opening a pull request

- Do not add `area:` or `size:` labels. A pull request carries no area, and `pr-triage.yml` sets the size from the lines changed.
- Do not set `status:waiting-on-review`; `pr-triage.yml` sets it when a pull request opens ready, or leaves draft, with no status.
- Add a `ci:` label only to get what it runs, and only when the change needs it: `ci:ios-simulator` or `ci:ios-device` for the iOS app, `ci:android-device` for an APK, `ci:android-emulator-tests`, `ci:host-playground-e2e`. Each one starts macOS or emulator jobs on every later push, so do not add them by default.
- Add `ci:no-changeset` only when the change ships nothing, as docs/RELEASE_PROCESS.md describes.
- To label the issue the pull request finishes, write `Closes #N` in the body. The issue gets its `area:` labels when the pull request merges.

## While it is open

- Keep one `status:` label, named for who acts next: `status:waiting-on-author` after requesting changes, `status:ready-to-merge` once approved, `status:blocked` when something else has to land first, with a comment saying what.
- Add a `needs:` label when work is blocked on something other than code: `needs:spec`, `needs:decision` or `needs:UI`. On a pull request, a `needs:` label or `status:blocked` fails `Label gate`, so remove it once resolved.

## Filing an issue

- Add `area:` labels for what it touches, and a `spa:` label when a product needs it.
- Type, status, priority and target date are issue fields, not labels.

## Adding, renaming or retiring a label

- Never create, rename or delete a label with `gh label` or the web UI. Edit `.github/labels.yml` in a pull request; `scripts/sync-labels.py` applies it when the pull request merges.
- Name it `family:value`, lowercase with hyphens, in an existing family. Give it a color, a description saying when to use it, a `target` and an `addedBy`.
- To rename, put the old name under `previously`; the label stays on everything that carries it. To delete, move it under `retired` with a reason. A label whose replacement is not decided goes under `pending`.
- If a workflow, script, template or doc names the label, change it in the same pull request, so the label and its users land together. `rg` for the old name across `.github`, `scripts`, `.claude` and the docs.
- Run `python3 scripts/sync-labels.py` to see the plan against the live labels. It only reads. Never pass `--apply`; the push to main does that.
- `diagnosis-report` keeps its name: deployed copies of the playground open issues with it.
