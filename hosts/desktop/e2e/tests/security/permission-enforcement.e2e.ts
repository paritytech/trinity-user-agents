import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Permission Enforcement', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('blocks camera access', { tag: ['@allure.id:14943'] }, ({ probeResults }) => {
    const result = probe(probeResults, 'perm.camera');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks microphone access', ({ probeResults }) => {
    const result = probe(probeResults, 'perm.microphone');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks geolocation access', ({ probeResults }) => {
    const result = probe(probeResults, 'perm.geolocation');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks notification permission', ({ probeResults }) => {
    const result = probe(probeResults, 'perm.notification');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks clipboard read', ({ probeResults }) => {
    const result = probe(probeResults, 'perm.clipboard.read');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('allows clipboard write (may be context-limited in test)', ({ probeResults }) => {
    const result = probe(probeResults, 'perm.clipboard.write');
    // Clipboard write permission is granted by the sandbox, but the API may fail
    // in test context due to missing user gesture or non-HTTPS origin.
    // The probe passes either way — it reports 'allowed' or 'context-limited'.
    expect(result.passed).toBe(true);
    expect(result.actual).toMatch(/^(allowed|context-limited)/);
  });
});
