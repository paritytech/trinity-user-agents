import { describe, expect, it, vi } from 'vitest';

import { resetExecutionEnvironment } from '@/shared/execution-environment';

import { createState } from './state';

describe('createState re-entrant writes', () => {
  it('leaves every subscriber on the value the nested write produced, not the outer one', () => {
    const state = createState({ phase: 'idle', flag: false });

    // Subscribed first, like a bootstrap-time watcher: it reacts to a phase change by
    // raising a flag on the same state.
    state.value$.subscribe(({ phase }) => {
      if (phase !== 'done') return;
      state.set(prev => (prev.flag ? prev : { ...prev, flag: true }));
    });

    // Subscribed second, like a component mounting later.
    const seen: boolean[] = [];
    state.value$.subscribe(({ flag }) => seen.push(flag));
    seen.length = 0;

    state.set(prev => ({ ...prev, phase: 'done' }));

    expect(seen.at(-1)).toBe(true);
    expect(state.get().flag).toBe(true);
  });

  it('resolves a queued updater against the result of the write ahead of it', () => {
    const state = createState(0);

    state.value$.subscribe(value => {
      if (value !== 1) return;
      state.set(prev => prev + 1);
      state.set(prev => prev + 1);
    });

    state.set(1);

    expect(state.get()).toBe(3);
  });

  it('contains a queued updater that throws: the outer writer does not see it and later queued writes still apply', () => {
    const errorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const state = createState(0);

    state.value$.subscribe(value => {
      if (value !== 1) return;
      state.set(() => {
        throw new Error('queued updater blew up');
      });
      state.set(prev => prev + 10);
    });

    expect(() => state.set(1)).not.toThrow();
    expect(state.get()).toBe(11);
    expect(errorSpy).toHaveBeenCalledOnce();

    errorSpy.mockRestore();
  });

  it('keeps accepting writes after an updater throws mid-emission', () => {
    const state = createState(0);

    expect(() =>
      state.set(() => {
        throw new Error('updater blew up');
      }),
    ).toThrow('updater blew up');

    state.set(2);
    expect(state.get()).toBe(2);
  });
});

describe('createState reset', () => {
  it('returns to its initial value on reset() and tells subscribers', () => {
    const state = createState(1);
    const seen: number[] = [];
    state.value$.subscribe(value => seen.push(value));

    state.set(5);
    state.reset();

    expect(state.get()).toBe(1);
    expect(seen).toEqual([1, 5, 1]);
  });

  it('resets when the execution environment resets', () => {
    const state = createState({ count: 0 });
    state.set({ count: 3 });

    resetExecutionEnvironment();

    expect(state.get()).toEqual({ count: 0 });
  });

  it('reset() is a no-op emission when already at the initial value', () => {
    const state = createState('a');
    const listener = vi.fn();
    state.value$.subscribe(listener);

    state.reset();

    // `value$` is `distinctUntilChanged`, so the only emission is the initial one.
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
