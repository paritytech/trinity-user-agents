import { describe, expect, it } from 'vitest';

import { computeAutoUpdateSupported } from './utils';

describe('computeAutoUpdateSupported', () => {
  it('is supported when the build enables it and an update feed is baked in', () => {
    expect(computeAutoUpdateSupported({ enabledInBuild: true, hasUpdateFeed: true })).toBe(true);
  });

  it('is not supported when the build does not enable it', () => {
    expect(computeAutoUpdateSupported({ enabledInBuild: false, hasUpdateFeed: true })).toBe(false);
  });

  it('is not supported without an update feed even when enabled', () => {
    expect(computeAutoUpdateSupported({ enabledInBuild: true, hasUpdateFeed: false })).toBe(false);
  });
});
