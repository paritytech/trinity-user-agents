import * as allure from 'allure-js-commons';

import { expect, probe, securityTest as test } from '../../fixtures/security';

test.describe('Network Isolation', { tag: ['@security'] }, () => {
  test.beforeEach(async () => {
    await allure.suite('Security');
    await allure.feature('Security');
  });

  test('blocks fetch to HTTPS endpoints', { tag: ['@allure.id:14942'] }, ({ probeResults }) => {
    const result = probe(probeResults, 'net.fetch.https');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks fetch to HTTP endpoints', ({ probeResults }) => {
    const result = probe(probeResults, 'net.fetch.http');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks XMLHttpRequest to external URLs', ({ probeResults }) => {
    const result = probe(probeResults, 'net.xhr');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks WebSocket wss connections', ({ probeResults }) => {
    const result = probe(probeResults, 'net.websocket');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks WebSocket ws (plain) connections', ({ probeResults }) => {
    const result = probe(probeResults, 'net.websocket.ws');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks EventSource connections', ({ probeResults }) => {
    const result = probe(probeResults, 'net.eventsource');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks dynamic script tags from external URLs', ({ probeResults }) => {
    const result = probe(probeResults, 'net.script');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks dynamic img tags from external URLs', ({ probeResults }) => {
    const result = probe(probeResults, 'net.img');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks dynamic link tags from external URLs', ({ probeResults }) => {
    const result = probe(probeResults, 'net.link');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks navigator.sendBeacon', ({ probeResults }) => {
    const result = probe(probeResults, 'net.beacon');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('blocks Worker from external URL', ({ probeResults }) => {
    const result = probe(probeResults, 'net.worker');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('blocked');
  });

  test('allows fetch to polkadot:// scheme (sanity check)', ({ probeResults }) => {
    const result = probe(probeResults, 'net.fetch.product');
    expect(result.passed).toBe(true);
    expect(result.actual).toContain('allowed');
  });
});
