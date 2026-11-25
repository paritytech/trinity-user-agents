import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Storage Isolation', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('localStorage is accessible within partition', { tag: ['@allure.id:14940'] }, ({ probeResults }) => {
    const result = probe(probeResults, 'store.localstorage');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('accessible');
  });

  test('sessionStorage is accessible within partition', ({ probeResults }) => {
    const result = probe(probeResults, 'store.sessionstorage');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('accessible');
  });

  test('IndexedDB is accessible within partition', ({ probeResults }) => {
    const result = probe(probeResults, 'store.indexeddb');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('accessible');
  });

  test('cookies are accessible or scheme-limited on polkadot://', ({ probeResults }) => {
    const result = probe(probeResults, 'store.cookie');
    // polkadot:// custom scheme doesn't support document.cookie,
    // so the probe reports 'scheme-limited' instead of 'accessible'.
    // Either result is acceptable — the important thing is it doesn't error.
    expect(result.passed).toBe(true);
    expect(result.actual).toMatch(/^(accessible|scheme-limited)/);
  });

  test('host app IndexedDB is not accessible from product partition', ({ probeResults }) => {
    const result = probe(probeResults, 'store.host_db');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('empty');
  });
});
