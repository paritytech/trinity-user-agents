// @vitest-environment happy-dom
// Importing this module wires the persisted version state to localStorage at load time, so the
// suite needs a DOM environment even though these cases only exercise the pure helpers.

import { describe, expect, it } from 'vitest';

import { getVersion, isVersionNewer } from './updateCheck';

describe('isVersionNewer', () => {
  it('is true when the major/minor/patch is greater', () => {
    expect(isVersionNewer('2.1.0', '2.0.0')).toBe(true);
    expect(isVersionNewer('3.0.0', '2.9.9')).toBe(true);
  });

  it('compares numeric segments, not lexically', () => {
    expect(isVersionNewer('2.10.0', '2.9.0')).toBe(true);
    expect(isVersionNewer('2.9.0', '2.10.0')).toBe(false);
  });

  it('is false for equal versions', () => {
    expect(isVersionNewer('2.1.0', '2.1.0')).toBe(false);
  });

  it('is false for older versions (no downgrade prompt)', () => {
    expect(isVersionNewer('2.0.0', '2.1.0')).toBe(false);
  });

  it('treats missing segments as zero', () => {
    expect(isVersionNewer('2.1', '2.1.0')).toBe(false);
    expect(isVersionNewer('2.1.1', '2.1')).toBe(true);
  });
});

describe('getVersion', () => {
  it('extracts a string version from IPC data', () => {
    expect(getVersion({ version: '2.1.0' })).toBe('2.1.0');
  });

  it('returns null for malformed data', () => {
    expect(getVersion(null)).toBeNull();
    expect(getVersion({ version: 5 })).toBeNull();
    expect(getVersion('nope')).toBeNull();
  });
});
