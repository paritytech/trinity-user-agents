import * as v from 'valibot';
import { describe, expect, it } from 'vitest';

import { genesisHash } from '../chain/schemas';

import { connectionService } from './service';
import { type ConnectionSettings } from './types';

const CHAIN = v.parse(genesisHash, '0x8c27ddf678c2ae9bef0efebfc485a9309f3d735c6d3fbb8d947afc3ace0e80f4');

// Whether the embedded light client carries a spec for the chain is an input to
// these helpers, not something they look up.
const AVAILABLE = true;
const UNAVAILABLE = false;

const settings = (overrides: Partial<ConnectionSettings> = {}): ConnectionSettings => ({
  preference: 'light-client',
  overrides: {},
  ...overrides,
});

describe('connectionService.resolveMode', () => {
  it('applies a non-advanced preference to every network', () => {
    expect(connectionService.resolveMode(settings(), CHAIN, AVAILABLE)).toBe('light-client');
    expect(connectionService.resolveMode(settings({ preference: 'rpc' }), CHAIN, AVAILABLE)).toBe('rpc');
  });

  it('ignores overrides while the preference is not advanced', () => {
    const withOverride = settings({ preference: 'rpc', overrides: { [CHAIN]: 'light-client' } });

    expect(connectionService.resolveMode(withOverride, CHAIN, AVAILABLE)).toBe('rpc');
  });

  it('honours a per-network override under advanced', () => {
    const advanced = settings({ preference: 'advanced', overrides: { [CHAIN]: 'rpc' } });

    expect(connectionService.resolveMode(advanced, CHAIN, AVAILABLE)).toBe('rpc');
  });

  it('starts an untouched network on the light client under advanced', () => {
    expect(connectionService.resolveMode(settings({ preference: 'advanced' }), CHAIN, AVAILABLE)).toBe('light-client');
  });

  it('degrades to rpc when the embedded client cannot serve the network', () => {
    expect(connectionService.resolveMode(settings(), CHAIN, UNAVAILABLE)).toBe('rpc');
    expect(
      connectionService.resolveMode(
        settings({ preference: 'advanced', overrides: { [CHAIN]: 'light-client' } }),
        CHAIN,
        UNAVAILABLE,
      ),
    ).toBe('rpc');
  });
});

describe('connectionService.describeChainMode', () => {
  it('separates what was requested from what will actually be used', () => {
    expect(connectionService.describeChainMode(settings(), CHAIN, UNAVAILABLE)).toEqual({
      requested: 'light-client',
      effective: 'rpc',
      lightClientAvailable: false,
    });
  });

  it('reports the override as the requested mode under advanced', () => {
    const advanced = settings({ preference: 'advanced', overrides: { [CHAIN]: 'rpc' } });

    expect(connectionService.describeChainMode(advanced, CHAIN, AVAILABLE)).toEqual({
      requested: 'rpc',
      effective: 'rpc',
      lightClientAvailable: true,
    });
  });
});

describe('connectionService.chainsAffectedBy', () => {
  const OTHER = v.parse(genesisHash, '0x91b171bb158e2d3848fa23a9f1c25182fb8e20313b2c1eb49219da7a70ce90c3');

  it('reports every network when the preference itself changes', () => {
    expect(connectionService.chainsAffectedBy(settings(), settings({ preference: 'rpc' }))).toBe('all');
  });

  it('reports nothing when neither the preference nor a consulted override moved', () => {
    expect(connectionService.chainsAffectedBy(settings(), settings())).toEqual([]);
  });

  it('ignores override edits made while the preference is not advanced', () => {
    const before = settings({ preference: 'rpc' });
    const after = settings({ preference: 'rpc', overrides: { [CHAIN]: 'light-client' } });

    expect(connectionService.chainsAffectedBy(before, after)).toEqual([]);
  });

  it('reports only the networks whose override actually changed', () => {
    const before = settings({ preference: 'advanced', overrides: { [CHAIN]: 'rpc', [OTHER]: 'rpc' } });
    const after = settings({ preference: 'advanced', overrides: { [CHAIN]: 'light-client', [OTHER]: 'rpc' } });

    expect(connectionService.chainsAffectedBy(before, after)).toEqual([CHAIN]);
  });

  it('reports a network whose override was dropped back to the advanced default', () => {
    const before = settings({ preference: 'advanced', overrides: { [CHAIN]: 'rpc' } });
    const after = settings({ preference: 'advanced' });

    expect(connectionService.chainsAffectedBy(before, after)).toEqual([CHAIN]);
  });
});
