import { describe, expect, it } from 'vitest';

import { UpgradeConsentSignalCodec } from './upgradeConsent';

describe('UpgradeConsentSignalCodec', () => {
  it.each([
    ['request', 0],
    ['accept', 1],
    ['decline', 2],
  ] as const)('encodes %s to a single discriminant byte %d', (tag, discriminant) => {
    const encoded = UpgradeConsentSignalCodec.enc({ tag, value: undefined });
    expect(Array.from(encoded)).toEqual([discriminant]);
  });

  it.each(['request', 'accept', 'decline'] as const)('round-trips %s', tag => {
    const decoded = UpgradeConsentSignalCodec.dec(UpgradeConsentSignalCodec.enc({ tag, value: undefined }));
    expect(decoded.tag).toBe(tag);
  });
});
