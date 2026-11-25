import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Node.js Context Isolation', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('require is not available', { tag: ['@allure.id:14941'] }, ({ probeResults }) => {
    const result = probe(probeResults, 'node.require');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('process is not available', ({ probeResults }) => {
    const result = probe(probeResults, 'node.process');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('global is not available', ({ probeResults }) => {
    const result = probe(probeResults, 'node.global');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('__dirname is not available', ({ probeResults }) => {
    const result = probe(probeResults, 'node.dirname');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('module is not available', ({ probeResults }) => {
    const result = probe(probeResults, 'node.module');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('window.App (host preload) is not exposed to products', ({ probeResults }) => {
    const result = probe(probeResults, 'ctx.window_app');
    expect(result.passed).toBe(true);
    expect(result.actual).toBe('undefined');
  });

  test('__HOST_WEBVIEW_MARK__ is exposed (sandbox preload)', ({ probeResults }) => {
    const result = probe(probeResults, 'ctx.webview_mark');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('exists');
  });

  test('electron module is not accessible', ({ probeResults }) => {
    const result = probe(probeResults, 'ctx.ipc_renderer');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });
});
