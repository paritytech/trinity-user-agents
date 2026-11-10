import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { FALLBACK_TLD, networkTld, productName } from './dotns';

describe('productName', () => {
  it('completes a label and leaves a completed name alone', () => {
    expect(productName('host-playground', '.paseo')).toBe('host-playground.paseo');
    expect(productName('host-playground.paseo', '.paseo')).toBe('host-playground.paseo');
  });
});

describe('networkTld', () => {
  it('answers each environment its own suffix', () => {
    expect(networkTld('nightly')).toBe('.paseo');
    expect(networkTld('unstable')).toBe(FALLBACK_TLD);
  });
});

// The override is read once at module load (through `e2eConfig`), so each case
// re-imports with the env stubbed rather than mutating a live value.
describe('E2E_DOTNS_TLD', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it('overrides every environment', async () => {
    vi.stubEnv('E2E_DOTNS_TLD', '.example');
    const dotns = await import('./dotns');

    expect(dotns.networkTld('nightly')).toBe('.example');
    expect(dotns.networkTld('unstable')).toBe('.example');
  });

  it('leaves the app fallback alone — it mirrors the app, it is not a knob', async () => {
    vi.stubEnv('E2E_DOTNS_TLD', '.example');
    const dotns = await import('./dotns');

    expect(dotns.FALLBACK_TLD).toBe('.dot');
  });

  it('rejects a value the chain could not have returned', async () => {
    vi.stubEnv('E2E_DOTNS_TLD', 'paseo');

    await expect(import('./dotns')).rejects.toThrow(/leading dot/);
  });
});
