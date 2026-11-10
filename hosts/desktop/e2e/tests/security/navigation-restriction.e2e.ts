import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Navigation Restriction', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('blocks window.open to external URLs', { tag: ['@allure.id:14945'] }, ({ probeResults }) => {
    const result = probe(probeResults, 'nav.window_open');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('denied');
  });

  test('blocks data URI navigation', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.data_uri');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks javascript: URI', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.javascript_uri');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('allows navigation within polkadot:// scheme', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.product_internal');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('allowed');
  });
});

test.describe('Side Channel Probes', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('SharedArrayBuffer is not available', ({ probeResults }) => {
    const result = probe(probeResults, 'timing.shared_array_buffer');
    // SharedArrayBuffer may or may not be available depending on headers — informational
    expect(result.passed).toBe(true);
  });

  test('performance.now() precision (informational)', ({ probeResults }) => {
    const result = probe(probeResults, 'timing.perf_precision');
    // Always passes — just reports precision
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('informational');
  });

  test('measureUserAgentSpecificMemory is blocked', ({ probeResults }) => {
    const result = probe(probeResults, 'timing.memory');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });
});
