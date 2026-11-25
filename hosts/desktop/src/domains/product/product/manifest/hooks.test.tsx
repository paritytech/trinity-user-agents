// @vitest-environment happy-dom

import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';

import { type Environment, environmentResource } from '@/domains/application';
import { type Product } from '../types';

import { useExecutableArchive, useLiveExecutable } from './hooks';
import { executableArchiveResource, liveExecutableResource } from './resource';

// The subject is the environment gate in front of each read: an unstarted read
// must not report settled.
const environmentAssembling = () => environmentResource.instead(() => new Promise<Environment>(() => {}));

const RESOLVED = { id: 'alpha', ipfsGatewayUrl: 'https://ipfs.example' } as unknown as Environment;
const environmentResolved = () => environmentResource.instead(() => RESOLVED);

// These reads must stay in flight: a settled one would answer the question the
// tests are asking about the gate.
beforeEach(() => {
  executableArchiveResource.instead(() => new Promise(() => {}));
  liveExecutableResource.instead(() => new Promise(() => {}));
});

const PRODUCT: Product = {
  baseName: 'app.dot',
  displayName: 'App',
  description: '',
  icon: { cid: 'abc', format: 'png' },
  executables: {},
};

describe('useLiveExecutable', () => {
  it('stays pending while the environment is still assembling', () => {
    // `useActiveEnvironment` returns null until Remote Config assembles the environment.
    environmentAssembling();

    const { result } = renderHook(() => useLiveExecutable({ product: PRODUCT, kind: 'app' }));

    // The underlying read is idle in this window and `useRead` reports idle as
    // `pending: false`. Surfacing that verbatim is indistinguishable from
    // "settled — no update available", so the environment's own wait must carry through.
    expect(result.current.pending).toBe(true);
    expect(result.current.data).toBeNull();
  });

  it('does not invent pending when no executable was asked for', () => {
    environmentAssembling();

    const { result } = renderHook(() => useLiveExecutable(null));

    // Nothing was requested, so there is nothing to wait for — claiming pending here
    // would stall callers that never wanted a read.
    expect(result.current.pending).toBe(false);
  });

  it('reports settled once the environment has resolved and the read is idle', () => {
    environmentResolved();

    const { result } = renderHook(() => useLiveExecutable(null));

    expect(result.current.pending).toBe(false);
  });
});

describe('useExecutableArchive', () => {
  it('stays pending while the environment is still assembling', () => {
    environmentAssembling();

    const { result } = renderHook(() => useExecutableArchive({ product: PRODUCT, kind: 'app' }));

    // Webview reads `!pending && !content` as "archive missing" and renders an
    // error, so an idle read must not surface as settled here.
    expect(result.current.pending).toBe(true);
  });

  it('does not invent pending when no archive was asked for', () => {
    environmentAssembling();

    const { result } = renderHook(() => useExecutableArchive(null));

    expect(result.current.pending).toBe(false);
  });

  // The read only starts in an effect, so the render where the environment lands
  // is one where nothing has started yet. `result.current` cannot see it (rerender
  // is act-wrapped), hence the per-render capture.
  it('never reports settled across the environment resolving', () => {
    environmentAssembling();

    const seen: boolean[] = [];
    const { rerender } = renderHook(() => {
      const state = useExecutableArchive({ product: PRODUCT, kind: 'app' });
      seen.push(state.pending);

      return state;
    });

    seen.length = 0;
    environmentResolved();
    rerender();

    expect(seen).not.toContain(false);
  });
});
