import { describe, expect, it, vi } from 'vitest';

import {
  getExecutionEnvironment,
  isTestEnvironment,
  onExecutionEnvironmentReset,
  resetExecutionEnvironment,
  setExecutionEnvironment,
} from './index';

describe('execution environment', () => {
  it('reports the environment it was set to', () => {
    setExecutionEnvironment('runtime');
    expect(getExecutionEnvironment()).toBe('runtime');
    expect(isTestEnvironment()).toBe(false);

    setExecutionEnvironment('test');
    expect(getExecutionEnvironment()).toBe('test');
    expect(isTestEnvironment()).toBe(true);
  });

  it('calls every listener once per reset', () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = onExecutionEnvironmentReset(first);
    const stopSecond = onExecutionEnvironmentReset(second);

    resetExecutionEnvironment();
    resetExecutionEnvironment();

    expect(first).toHaveBeenCalledTimes(2);
    expect(second).toHaveBeenCalledTimes(2);
    stopFirst();
    stopSecond();
  });

  it('stops calling a listener once it unsubscribes', () => {
    const listener = vi.fn();
    const stop = onExecutionEnvironmentReset(listener);

    resetExecutionEnvironment();
    stop();
    resetExecutionEnvironment();

    expect(listener).toHaveBeenCalledTimes(1);
  });
});
