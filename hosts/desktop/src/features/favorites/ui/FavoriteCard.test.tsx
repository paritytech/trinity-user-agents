// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { FavoriteCard } from './FavoriteCard';

describe('FavoriteCard', () => {
  it('renders the title and the action node', () => {
    render(<FavoriteCard title="Coin Flip" action={<span>ACTION</span>} onOpen={vi.fn()} />);
    expect(screen.getByText('Coin Flip')).toBeInTheDocument();
    expect(screen.getByText('ACTION')).toBeInTheDocument();
  });

  it('calls onOpen when the card body is clicked', () => {
    const onOpen = vi.fn();
    render(<FavoriteCard title="Coin Flip" onOpen={onOpen} />);
    screen.getByRole('button', { name: 'Coin Flip' }).click();
    expect(onOpen).toHaveBeenCalledOnce();
  });
});
