// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { getAvatarPalette } from '@/shared/utils';

import { CallAvatar } from './CallAvatar';

describe('CallAvatar', () => {
  it('renders the uppercased first character of name', () => {
    render(<CallAvatar name="alice" />);
    expect(screen.getByText('A')).toBeDefined();
  });

  it('renders the uppercased first character for already-uppercase name', () => {
    render(<CallAvatar name="Bob" />);
    expect(screen.getByText('B')).toBeDefined();
  });

  it('renders without crash with default sizePx', () => {
    const { container } = render(<CallAvatar name="charlie" />);
    expect(container.firstChild).toBeDefined();
  });

  it('renders without crash with custom sizePx', () => {
    const { container } = render(<CallAvatar name="dave" sizePx={120} />);
    expect(container.firstChild).toBeDefined();
  });

  it('uses the same per-name palette colours as the chat list avatar', () => {
    render(<CallAvatar name="mysticRiver.88" />);
    const el = screen.getByText('M'); // the avatar div (letter is its direct child)
    const palette = getAvatarPalette('mysticRiver.88');
    expect(el.style.backgroundColor).toBe(palette.bg);
    expect(el.style.color).toBe(palette.fg);
  });
});
