import { describe, expect, it, vi } from 'vitest';

import { guarded, range } from './functions';

describe('range', () => {
  it('should yield ascending values including both bounds', () => {
    expect([...range(1, 5)]).toEqual([1, 2, 3, 4, 5]);
  });

  it('should yield descending values including both bounds', () => {
    expect([...range(5, 1)]).toEqual([5, 4, 3, 2, 1]);
  });

  it('should yield both values when bounds differ by one', () => {
    expect([...range(0, 1)]).toEqual([0, 1]);
    expect([...range(1, 0)]).toEqual([1, 0]);
  });

  it('should yield a single value when bounds are equal', () => {
    expect([...range(0, 0)]).toEqual([0]);
    expect([...range(7, 7)]).toEqual([7]);
    expect([...range(-3, -3)]).toEqual([-3]);
  });

  it('should cross zero in both directions', () => {
    expect([...range(-2, 3)]).toEqual([-2, -1, 0, 1, 2, 3]);
    expect([...range(3, -2)]).toEqual([3, 2, 1, 0, -1, -2]);
  });

  it('should work entirely in the negative range', () => {
    expect([...range(-5, -1)]).toEqual([-5, -4, -3, -2, -1]);
    expect([...range(-1, -5)]).toEqual([-1, -2, -3, -4, -5]);
  });

  it('should be a generator usable in for..of', () => {
    const collected: number[] = [];
    for (const i of range(1, 4)) {
      collected.push(i);
    }
    expect(collected).toEqual([1, 2, 3, 4]);
  });

  it('should produce an independent iterator on each invocation', () => {
    const a = range(0, 3);
    const b = range(0, 3);
    expect(a.next().value).toBe(0);
    expect(a.next().value).toBe(1);
    expect(b.next().value).toBe(0);
    expect([...a]).toEqual([2, 3]);
    expect([...b]).toEqual([1, 2, 3]);
  });
});

describe('guarded', () => {
  it('forwards args and returns the inner result while alive', () => {
    const fn = vi.fn((n: number) => n * 2);
    const inst = { disposed: false };
    const wrapped = guarded(inst, fn);

    expect(wrapped(3)).toBe(6);
    expect(fn).toHaveBeenCalledWith(3);
  });

  it('no-ops after disposed flips to true', () => {
    const fn = vi.fn();
    const inst = { disposed: false };
    const wrapped = guarded(inst, fn);

    inst.disposed = true;
    const result = wrapped();

    expect(fn).not.toHaveBeenCalled();
    expect(result).toBeUndefined();
  });

  it('reads disposed at call time, not at wrap time', () => {
    const fn = vi.fn();
    const inst = { disposed: true };
    const wrapped = guarded(inst, fn);

    inst.disposed = false;
    wrapped();

    expect(fn).toHaveBeenCalledTimes(1);
  });
});
