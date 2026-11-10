import { e2eConfig } from '../config';

import { type E2eEnvironmentId } from './environment';

/**
 * What the app falls back to when a deployment reports no TLD —
 * `DEFAULT_DOTNS_TLD` in `domains/product/dotns/constants.ts`. Mirrors the app
 * constant, so it is deliberately not overridable.
 */
export const FALLBACK_TLD = '.dot';

/**
 * The environment onboarding starts on, which the no-auth projects (`browser`,
 * `link-navigation`) keep — they skip the picker rather than choosing, so the
 * default stays selected and the app resolves a TLD there like anywhere else.
 * Verified: with nothing signed in, the browser project's address bar reads
 * "Enter .paseo address".
 */
export const DEFAULT_ENVIRONMENT_ID: E2eEnvironmentId = 'nightly';

/**
 * The dotNS suffix each environment serves. Since paritytech/dotns#201 it is
 * per deployment, read from `DotnsProtocolRegistry.tld()`, so one product is a
 * different name on different environments — which is why a test names a
 * product by its label and completes it through `productName`.
 *
 * `E2E_DOTNS_TLD` overrides this for a run; edit the map when a deployment
 * changes its TLD for good.
 */
const ENV_TO_DOTNS_TLD: Record<E2eEnvironmentId, string> = {
  // `DotnsProtocolRegistry.tld()` on Paseo Next V2 (registry
  // 0xf34054fd76BbF85f216cf9908226D5f0A72E50CA) answers ".paseo".
  nightly: '.paseo',
  // PreviewNet predates the per-network TLD and answers with empty data, so the
  // app falls back.
  unstable: FALLBACK_TLD,
};

// The app validates the on-chain value with the same shape (`networkTldSchema`),
// so an override that could not have come off the chain is a typo, not a config.
const TLD_SHAPE = /^\.[a-z0-9][a-z0-9-]{0,62}$/;

function readOverride(): string | undefined {
  const configured = e2eConfig.dotNsTld;
  if (!configured) return undefined;
  if (!TLD_SHAPE.test(configured)) {
    throw new Error(`E2E_DOTNS_TLD must be a leading dot plus one DNS label (e.g. ".paseo"), got "${configured}"`);
  }

  return configured;
}

const TLD_OVERRIDE = readOverride();

export function networkTld(envId: E2eEnvironmentId): string {
  return TLD_OVERRIDE ?? ENV_TO_DOTNS_TLD[envId];
}

/** `host-playground` → `host-playground.paseo` on nightly. Idempotent. */
export function productName(label: string, tld: string): string {
  return label.endsWith(tld) ? label : `${label}${tld}`;
}
