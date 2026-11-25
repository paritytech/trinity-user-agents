import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { buildIceConfig } from './iceConfig';

// Freeze time so the derived expiry timestamp (and thus the HMAC) is deterministic.
const NOW_MS = 1_700_000_000_000;
const TTL_SECONDS = 86_400;
const SECRET = 's3cr3t';
const EXPECTED_CREDENTIALS = 'Wbg+b0b85AFdWW1nm5dkKnEW/vA=';

function turnServerOf(config: RTCConfiguration): RTCIceServer | undefined {
  return config.iceServers?.find(s => (Array.isArray(s.urls) ? s.urls : [s.urls]).some(u => u.startsWith('turn')));
}

describe('buildIceConfig', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW_MS);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('always includes Google public STUN servers', () => {
    const config = buildIceConfig({});
    const stunServers = config.iceServers?.filter(s =>
      (Array.isArray(s.urls) ? s.urls : [s.urls]).some(u => u.startsWith('stun:')),
    );
    expect(stunServers?.length).toBeGreaterThan(0);
  });

  it('derives TURN REST credentials from the shared secret', () => {
    const config = buildIceConfig({ turnHost: 'turn.example.com', turnSecret: SECRET });
    const turnServer = turnServerOf(config);

    const expectedUsername = String(Math.floor(NOW_MS / 1000) + TTL_SECONDS);
    expect(turnServer?.username).toBe(expectedUsername);
    expect(turnServer?.credential).toBe(EXPECTED_CREDENTIALS);
  });

  it('honours a custom credential lifetime', () => {
    const ttl = 3_600;
    const config = buildIceConfig({ turnHost: 'turn.example.com', turnSecret: SECRET, turnTtlSeconds: ttl });
    const turnServer = turnServerOf(config);

    const expectedUsername = String(Math.floor(NOW_MS / 1000) + ttl);
    expect(turnServer?.username).toBe(expectedUsername);
    expect(turnServer?.credential).toBe('TlAMeu/gIhBIQSwpz40UsUQVgmw=');
  });

  it('omits the TURN server when host or secret is missing', () => {
    expect(turnServerOf(buildIceConfig({}))).toBeUndefined();
    expect(turnServerOf(buildIceConfig({ turnHost: 'turn.example.com' }))).toBeUndefined();
    expect(turnServerOf(buildIceConfig({ turnSecret: SECRET }))).toBeUndefined();
  });

  it('emits UDP, TCP, and TLS transports under a single credential', () => {
    const config = buildIceConfig({ turnHost: 'turn.example.com', turnSecret: SECRET });
    const turnServer = turnServerOf(config);
    const urls = Array.isArray(turnServer?.urls) ? turnServer.urls : [turnServer?.urls];
    expect(urls).toEqual([
      'turn:turn.example.com:3478?transport=udp',
      'turn:turn.example.com:3478?transport=tcp',
      'turns:turn.example.com:5349',
    ]);
    // One entry, so the derived credential applies to every transport.
    const expectedUsername = String(Math.floor(NOW_MS / 1000) + TTL_SECONDS);
    expect(turnServer?.username).toBe(expectedUsername);
    expect(turnServer?.credential).toBe(EXPECTED_CREDENTIALS);
  });
});
