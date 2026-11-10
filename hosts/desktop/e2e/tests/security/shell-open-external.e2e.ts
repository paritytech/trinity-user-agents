import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Shell Open External — Dangerous Protocol URLs', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('blocks window.open with file:// URL', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.open_file');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('denied');
  });

  test('blocks window.open with tel: URL', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.open_tel');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('denied');
  });

  test('blocks window.open with javascript: URL', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.open_javascript');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('denied');
  });

  test('blocks window.open with data: URL', ({ probeResults }) => {
    const result = probe(probeResults, 'nav.open_data');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('denied');
  });
});
