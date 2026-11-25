/**
 * E2E test configuration.
 * Single source of truth for environment-specific values.
 * Override via environment variables in CI or locally.
 *
 * Signer identity is not configured here: it is addressed by `--base-path` per
 * (project, worker) slot — see `e2e/helpers/signing-host.ts`.
 */
export const e2eConfig = {
  /**
   * Overrides the dotNS suffix for every environment, e.g. `E2E_DOTNS_TLD=.paseo`.
   * Unset means each environment uses its known suffix (`helpers/dotns.ts`) — set
   * it when a deployment changes its TLD before that map catches up.
   */
  dotNsTld: process.env['E2E_DOTNS_TLD'],
} as const;
