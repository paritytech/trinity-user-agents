export default {
  '{src,main}/**/*.{ts,tsx}': () => 'npm run types',
  // `prettier` directly, NOT `npm run fmt:generic --check`: npm consumes leading
  // `--flags` as its own config and never forwards them to the script, so that
  // form ran a bare `prettier <file>`, which prints the formatted text to stdout
  // and exits 0 — the check passed unconditionally and unformatted files reached
  // CI, where `fmt:check` failed. `--write` fixes instead of rejecting; lint-staged
  // re-stages whatever the task modifies.
  '*.{js,ts,tsx}': ['prettier --write', 'npm run lint:generic'],
};
