---
paths:
  - "rust/**/*.rs"
  - "rust/**/Cargo.toml"
  - "Cargo.toml"
---

# Rust design rules

Before writing or changing Rust, read `docs/rust-design.md` and follow it. It
is this repo's adaptation of the Google Rust style guide and covers design,
not formatting.

Before declaring a Rust change done, run the `rust-design-reviewer` agent on
the diff and resolve or justify every finding it reports.
