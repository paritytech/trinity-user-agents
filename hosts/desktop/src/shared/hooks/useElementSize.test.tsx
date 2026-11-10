// @vitest-environment happy-dom

import { render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useElementSize } from './useElementSize';

const observe = vi.fn();
const disconnect = vi.fn();

const Probe = () => {
  const { ref, height } = useElementSize<HTMLDivElement>();
  return <div ref={ref} data-height={height} />;
};

describe('useElementSize', () => {
  beforeEach(() => {
    observe.mockClear();
    disconnect.mockClear();
    vi.stubGlobal(
      'ResizeObserver',
      class {
        observe = observe;
        unobserve = vi.fn();
        disconnect = disconnect;
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('defaults to zero height before layout', () => {
    const { container } = render(<Probe />);

    const probe = container.firstElementChild;
    expect(probe?.getAttribute('data-height')).toBe('0');
  });

  it('observes the element and disconnects on unmount', () => {
    const { unmount } = render(<Probe />);

    expect(observe).toHaveBeenCalledTimes(1);

    unmount();

    expect(disconnect).toHaveBeenCalledTimes(1);
  });
});
