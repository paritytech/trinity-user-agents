// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { DismissOverlay } from './DismissOverlay';

const TEST_ID = 'overlay';

describe('DismissOverlay', () => {
  it('renders nothing when closed', () => {
    render(<DismissOverlay open={false} testId={TEST_ID} onDismiss={vi.fn()} />);
    expect(screen.queryByTestId(TEST_ID)).toBeNull();
  });

  it('renders the overlay when open', () => {
    render(<DismissOverlay open testId={TEST_ID} onDismiss={vi.fn()} />);
    expect(screen.getByTestId(TEST_ID)).toBeInTheDocument();
  });

  it('calls onDismiss on pointer down', () => {
    const onDismiss = vi.fn();
    render(<DismissOverlay open testId={TEST_ID} onDismiss={onDismiss} />);
    fireEvent.pointerDown(screen.getByTestId(TEST_ID));
    expect(onDismiss).toHaveBeenCalledTimes(1);
  });
});
