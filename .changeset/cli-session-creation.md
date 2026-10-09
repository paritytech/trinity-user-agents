---
"@parity/truapi": minor
---

Restore or provision explicitly selected CLI sessions, retry unfinished setup,
preserve imported account identity when switching sessions, and reject derived
username bases with fewer than six lowercase ASCII letters before onboarding.
Use `--session` to select the newest local session for a username base or an
exact session by full username. The separate username-prefix option is removed.
