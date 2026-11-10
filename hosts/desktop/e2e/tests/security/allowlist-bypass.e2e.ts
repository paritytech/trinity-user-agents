import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

/**
 * These tests verify that the allowlist uses exact domain matching,
 * preventing bypass via attacker-controlled domains that contain
 * allowlisted substrings (e.g., `ipfs.evil.com`, `saturn.evil.com`).
 */
test.describe('Allowlist Bypass', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('blocks fetch to fake IPFS domain', ({ probeResults }) => {
    const result = probe(probeResults, 'allow.ipfs.fake');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks fetch to IPFS subdomain abuse', ({ probeResults }) => {
    const result = probe(probeResults, 'allow.ipfs.subdomain');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks fetch to fake TURN domain', ({ probeResults }) => {
    const result = probe(probeResults, 'allow.turn.fake');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks fetch to saturn domain (contains "turn")', ({ probeResults }) => {
    const result = probe(probeResults, 'allow.saturn');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks fetch to fake STUN domain', ({ probeResults }) => {
    const result = probe(probeResults, 'allow.stun.fake');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });
});
