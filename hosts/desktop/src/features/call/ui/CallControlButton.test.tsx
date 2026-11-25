// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { CallControlButton } from './CallControlButton';

describe('CallControlButton', () => {
  it('renders the label', () => {
    render(<CallControlButton label="Accept" tone="accept" icon={<span>icon</span>} onPress={vi.fn()} />);
    expect(screen.getByText('Accept')).toBeDefined();
  });

  it('calls onPress when clicked', async () => {
    const onPress = vi.fn();
    render(<CallControlButton label="End" tone="decline" icon={<span>icon</span>} onPress={onPress} />);
    await userEvent.click(screen.getByRole('button'));
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it('renders with accept tone (green button)', () => {
    render(<CallControlButton label="Accept" tone="accept" icon={<span>icon</span>} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('bg-green');
  });

  it('renders with decline tone (error status button)', () => {
    render(<CallControlButton label="Decline" tone="decline" icon={<span>icon</span>} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('bg-bg-status-error');
  });

  it('renders with neutral tone', () => {
    render(<CallControlButton label="Mute" tone="neutral" icon={<span>icon</span>} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('bg-white');
  });

  it('renders with primary tone (light action button)', () => {
    render(<CallControlButton label="Camera" tone="primary" icon={<span>icon</span>} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('bg-bg-action-primary');
  });

  it('renders with secondary tone (dark action button)', () => {
    render(<CallControlButton label="Mute" tone="secondary" icon={<span>icon</span>} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('bg-bg-action-secondary');
  });

  it('renders a larger circle for size="lg"', () => {
    render(<CallControlButton label="End" tone="decline" size="lg" icon={<span>icon</span>} onPress={vi.fn()} />);
    expect(screen.getByRole('button').className).toContain('size-16');
  });

  it('applies reduced opacity when active is false', () => {
    render(<CallControlButton label="Video" tone="neutral" icon={<span>icon</span>} active={false} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).toContain('opacity-50');
  });

  it('does not apply opacity-50 when active is true', () => {
    render(<CallControlButton label="Video" tone="neutral" icon={<span>icon</span>} active={true} onPress={vi.fn()} />);
    const button = screen.getByRole('button');
    expect(button.className).not.toContain('opacity-50');
  });
});
