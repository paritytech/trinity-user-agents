/**
 * Maps the desktop's Environment id (the `VITE_ENVIRONMENTS` channel key, used as the
 * `network-button-<id>` testid by the onboarding picker) to the network name
 * `truapi-host` accepts for `--network`.
 *
 * The catalog's channel keys are `nightly` (display "Paseo Next V2") and `unstable`
 * (display "PreviewNet"); the networks they map to are `paseo-next-v2` and `previewnet`.
 */

export type E2eEnvironmentId = 'nightly' | 'unstable';

const ENV_TO_NETWORK: Record<E2eEnvironmentId, string> = {
  nightly: 'paseo-next-v2',
  unstable: 'previewnet',
};

export function envToNetwork(envId: E2eEnvironmentId): string {
  return ENV_TO_NETWORK[envId];
}
