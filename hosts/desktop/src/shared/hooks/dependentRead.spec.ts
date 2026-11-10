import { describe, expect, it } from 'vitest';

import { dependentRead } from './dependentRead';

const settled = { data: null, pending: false, error: null, fulfilled: false, refresh: () => {} };

describe('dependentRead', () => {
  it('reports pending while the dependency is loading', () => {
    const result = dependentRead(settled, { pending: true, error: null }, { enabled: true });

    expect(result.pending).toBe(true);
  });

  it('reports settled once the dependency has resolved and the read is idle', () => {
    const result = dependentRead(settled, { pending: false, error: null }, { enabled: true });

    expect(result.pending).toBe(false);
  });

  // A read whose dependency is loading again (a refetch) holds a result for params
  // that may be about to change.
  it('is not fulfilled while the dependency is loading', () => {
    const fulfilled = { ...settled, fulfilled: true };
    const result = dependentRead(fulfilled, { pending: true, error: null }, { enabled: true });

    expect(result.fulfilled).toBe(false);
  });

  it('keeps a fulfilled read fulfilled once the dependency has resolved', () => {
    const fulfilled = { ...settled, fulfilled: true };
    const result = dependentRead(fulfilled, { pending: false, error: null }, { enabled: true });

    expect(result.fulfilled).toBe(true);
  });

  it('stays pending while the read itself is in flight', () => {
    const inFlight = { ...settled, pending: true };
    const result = dependentRead(inFlight, { pending: false, error: null }, { enabled: true });

    expect(result.pending).toBe(true);
  });

  it('does not wait when nothing was requested', () => {
    const result = dependentRead(settled, { pending: true, error: new Error('boom') }, { enabled: false });

    expect(result.pending).toBe(false);
    expect(result.error).toBeNull();
  });

  it("surfaces the dependency's error when the read has none of its own", () => {
    const boom = new Error('environment unavailable');
    const result = dependentRead(settled, { pending: false, error: boom }, { enabled: true });

    expect(result.error).toBe(boom);
  });

  it("keeps the read's own error in preference to the dependency's", () => {
    const own = new Error('read failed');
    const errored = { ...settled, error: own };
    const result = dependentRead(errored, { pending: false, error: new Error('other') }, { enabled: true });

    expect(result.error).toBe(own);
  });
});
