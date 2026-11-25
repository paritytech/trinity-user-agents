import { Bytes, Enum, toHex } from '@novasamatech/scale';
import { Struct } from 'scale-ts';
import { describe, expect, it } from 'vitest';

import { consumerIdentityService } from './service';

const encoder = new TextEncoder();

const identifierKeyCodec = Enum({
  X25519: Struct({ key: Bytes(32), padding: Bytes(32) }),
});

const CHAT_KEY = new Uint8Array(32).fill(7);
// Hex, because that is the shape a fixed-width field arrives in — the variable-width
// username fields beside it stay bytes. See `schemas.ts`.
const encodedKey = toHex(identifierKeyCodec.enc({ tag: 'X25519', value: { key: CHAT_KEY, padding: new Uint8Array(32) } }));

function row(overrides: Record<string, unknown> = {}) {
  return {
    lite_username: encoder.encode('alice'),
    full_username: null,
    identifier_key: encodedKey,
    credibility: { type: 'Lite' },
    ...overrides,
  };
}

describe('consumerIdentityService.toConsumerIdentity', () => {
  it('decodes a lite record and unwraps the chat key', () => {
    const identity = consumerIdentityService.toConsumerIdentity('acc-1', row());

    expect(identity).toEqual({
      accountId: 'acc-1',
      liteUsername: 'alice',
      fullUsername: null,
      credibility: { type: 'Lite' },
      identifierKey: `0x${'07'.repeat(32)}`,
    });
  });

  it('prefers the full username when the chain carries one', () => {
    const identity = consumerIdentityService.toConsumerIdentity('acc-1', row({ full_username: encoder.encode('alice.dot') }));

    expect(identity?.fullUsername).toBe('alice.dot');
    expect(identity?.liteUsername).toBe('alice');
  });

  it('reads an attested record as Person', () => {
    const identity = consumerIdentityService.toConsumerIdentity(
      'acc-1',
      row({ credibility: { type: 'Person', value: { alias: '0xabcd', last_update: 42n } } }),
    );

    expect(identity?.credibility).toEqual({ type: 'Person', alias: '0xabcd', lastUpdate: '42' });
  });

  // A keypair variant this host cannot encrypt to is a normal condition — the
  // account still has a username worth showing.
  it('keeps the username when the chat key cannot be decoded', () => {
    const identity = consumerIdentityService.toConsumerIdentity('acc-1', row({ identifier_key: '0x090909' }));

    expect(identity?.identifierKey).toBeNull();
    expect(identity?.liteUsername).toBe('alice');
  });

  it('degrades a non-hex alias to Lite rather than surfacing it', () => {
    const identity = consumerIdentityService.toConsumerIdentity(
      'acc-1',
      row({ credibility: { type: 'Person', value: { alias: 'not-hex', last_update: null } } }),
    );

    expect(identity?.credibility).toEqual({ type: 'Lite' });
  });

  it('is null for an account with no record', () => {
    expect(consumerIdentityService.toConsumerIdentity('acc-1', undefined)).toBeNull();
    expect(consumerIdentityService.toConsumerIdentity('acc-1', { lite_username: 'not-bytes' })).toBeNull();
  });
});
