import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { findRegistrations, identityBackendUrl, matchesBase } from './identity-backend';

const REGISTRY = 'https://identity.example.test';

const row = (username: string, createdAt = '2026-09-15T21:33:13.661656Z') => ({
  accountId: '5Fuy9odejBvSPvJxwWcSfbfxfTSGZozK6QRwb7JVrxndvydj',
  username,
  status: 'ASSIGNED',
  createdAt,
});

const jsonResponse = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

beforeEach(() => {
  vi.stubEnv('E2E_IDENTITY_URL', REGISTRY);
  // These paths warn on every fallback; the assertions are the return values.
  vi.spyOn(console, 'warn').mockImplementation(() => {});
  vi.spyOn(console, 'info').mockImplementation(() => {});
});

afterEach(() => {
  vi.unstubAllEnvs();
});

describe('matchesBase', () => {
  it('accepts the bare name and the backend-assigned digit suffix', () => {
    expect(matchesBase('testbotaaaaaaaaaa', 'testbotaaaaaaaaaa')).toBe(true);
    expect(matchesBase('testbotaaaaaaaaaa.23', 'testbotaaaaaaaaaa')).toBe(true);
    expect(matchesBase('testbotaaaaaaaaaa.05', 'testbotaaaaaaaaaa')).toBe(true);
  });

  it('rejects a longer name that merely starts with the base', () => {
    // The endpoint matches by prefix, so this row really does come back.
    expect(matchesBase('testbotaaaaaaaaaabb', 'testbotaaaaaaaaaa')).toBe(false);
    expect(matchesBase('testbotaaaaaaaaaabb.07', 'testbotaaaaaaaaaa')).toBe(false);
  });

  it('rejects a non-numeric or empty suffix', () => {
    expect(matchesBase('testbotaaaaaaaaaa.', 'testbotaaaaaaaaaa')).toBe(false);
    expect(matchesBase('testbotaaaaaaaaaa.ab', 'testbotaaaaaaaaaa')).toBe(false);
  });

  it('rejects an unrelated name', () => {
    expect(matchesBase('desktopauthdalinux.05', 'testbotaaaaaaaaaa')).toBe(false);
  });
});

describe('identityBackendUrl', () => {
  it('is whatever the environment says', () => {
    expect(identityBackendUrl()).toBe(REGISTRY);
  });

  it('is undefined when unset, so no registry is assumed', () => {
    vi.stubEnv('E2E_IDENTITY_URL', '');
    expect(identityBackendUrl()).toBeUndefined();
  });
});

describe('findRegistrations', () => {
  it('returns the rows that are registrations of the base', async () => {
    const doFetch = vi.fn().mockResolvedValue(
      jsonResponse({
        usernames: [row('testbotaaaaaaaaaa.23'), row('testbotaaaaaaaaaabb.07'), row('testbotaaaaaaaaaa')],
      }),
    );

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toEqual([
      row('testbotaaaaaaaaaa.23'),
      row('testbotaaaaaaaaaa'),
    ]);
  });

  it('searches the configured registry by prefix', async () => {
    const doFetch = vi.fn().mockResolvedValue(jsonResponse({ usernames: [] }));
    await findRegistrations('testbotaaaaaaaaaa', doFetch);

    const [url] = doFetch.mock.calls[0] ?? [];
    expect(url).toContain(`${REGISTRY}/api/v1/usernames/search`);
    expect(url).toContain('prefix=testbotaaaaaaaaaa');
  });

  it('reports an empty registry answer as "not registered", not as "cannot tell"', async () => {
    const doFetch = vi.fn().mockResolvedValue(jsonResponse({ usernames: [] }));

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toEqual([]);
  });

  it('cannot tell when no registry is configured, and does not call out', async () => {
    vi.stubEnv('E2E_IDENTITY_URL', '');
    const doFetch = vi.fn();

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toBeNull();
    expect(doFetch).not.toHaveBeenCalled();
  });

  it('cannot tell when the deployment demands a proof-of-compute header', async () => {
    const doFetch = vi.fn().mockResolvedValue(jsonResponse({ error: 'PAYMENT_REQUIRED' }, 402));

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toBeNull();
  });

  it('cannot tell when the registry is rate-limiting or erroring', async () => {
    for (const status of [429, 500, 503]) {
      const doFetch = vi.fn().mockResolvedValue(jsonResponse({ error: 'nope' }, status));
      await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toBeNull();
    }
  });

  it('cannot tell when the request fails outright', async () => {
    const doFetch = vi.fn().mockRejectedValue(new Error('ECONNRESET'));

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toBeNull();
  });

  it('cannot tell when the payload is not the documented envelope', async () => {
    const doFetch = vi.fn().mockResolvedValue(jsonResponse([row('testbotaaaaaaaaaa.23')]));

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toBeNull();
  });

  it('drops rows missing a field it reads rather than yielding a half-read row', async () => {
    const doFetch = vi.fn().mockResolvedValue(
      jsonResponse({
        usernames: [{ username: 'testbotaaaaaaaaaa.23' }, row('testbotaaaaaaaaaa.24')],
      }),
    );

    await expect(findRegistrations('testbotaaaaaaaaaa', doFetch)).resolves.toEqual([row('testbotaaaaaaaaaa.24')]);
  });

  it('returns the newest registration first, whatever order the backend used', async () => {
    // A permanent identity accumulates one registration per heal; the backend orders
    // them by assigned digits, and only the newest belongs to the current account.
    const doFetch = vi.fn().mockResolvedValue(
      jsonResponse({
        usernames: [
          row('desktopauthdalinux.05', '2026-09-15T17:06:29.903102Z'),
          row('desktopauthdalinux.19', '2026-09-16T08:01:11.755676Z'),
          row('desktopauthdalinux.16', '2026-09-15T20:13:39.787418Z'),
        ],
      }),
    );

    const registrations = await findRegistrations('desktopauthdalinux', doFetch);

    expect(registrations?.map(entry => entry.username)).toEqual([
      'desktopauthdalinux.19',
      'desktopauthdalinux.16',
      'desktopauthdalinux.05',
    ]);
  });

  it('sorts a row with an unreadable timestamp last rather than letting it win', async () => {
    const doFetch = vi.fn().mockResolvedValue(
      jsonResponse({
        usernames: [row('testbotaaaaaaaaaa.05', 'not-a-date'), row('testbotaaaaaaaaaa.19', '2026-09-16T08:01:11.755676Z')],
      }),
    );

    const registrations = await findRegistrations('testbotaaaaaaaaaa', doFetch);

    expect(registrations?.[0]?.username).toBe('testbotaaaaaaaaaa.19');
  });
});
