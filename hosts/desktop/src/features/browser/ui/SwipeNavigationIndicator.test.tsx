// @vitest-environment happy-dom
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TEST_IDS } from '@/shared/test-ids';

import { SwipeNavigationIndicator } from './SwipeNavigationIndicator';

describe('SwipeNavigationIndicator', () => {
  it('renders nothing without a gesture', () => {
    const { container } = render(<SwipeNavigationIndicator indicator={null} />);
    expect(container).toBeEmptyDOMElement();
  });

  it.each([
    ['back', 'left'],
    ['forward', 'right'],
  ] as const)('pins the %s arrow to the %s edge', (direction, edge) => {
    render(<SwipeNavigationIndicator indicator={{ direction, progress: 0.5 }} />);
    const el = screen.getByTestId(TEST_IDS.swipeNavigationIndicator);
    expect(el).toHaveAttribute('data-edge', edge);
    expect(el).toHaveAttribute('aria-hidden', 'true');
    expect(el).toHaveAttribute('data-complete', 'false');
  });

  it('grows with progress', () => {
    render(<SwipeNavigationIndicator indicator={{ direction: 'back', progress: 0.5 }} />);
    expect(screen.getByTestId(TEST_IDS.swipeNavigationIndicator).style.transform).toContain('scale(0.8)');
  });

  it('marks the arrow complete at full progress', () => {
    render(<SwipeNavigationIndicator indicator={{ direction: 'back', progress: 1 }} />);
    expect(screen.getByTestId(TEST_IDS.swipeNavigationIndicator)).toHaveAttribute('data-complete', 'true');
  });
});
