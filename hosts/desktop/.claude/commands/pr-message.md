---
description: Draft a PR message from `.github/PULL_REQUEST_TEMPLATE.md` using main diff + uncommitted changes; print and `pbcopy`. Does NOT create a PR.
---

Create a Pull Request message for github. Use this template `.github/PULL_REQUEST_TEMPLATE.md`.
PR message should contain initial problem definition and simplified solution description.
Add code examples if necessary.
Be short, use straight simple language.
Use diff from main as well as all uncommitted changes.
Output the message to the console with markdown formatting for copy-pasting.
Call `pbcopy` command after outputing the message.

ATTENTION: DO NOT CREATE ACTUAL PULL REQUESTS! JUST WRITE THE MESSAGE TO THE OUTPUT.
