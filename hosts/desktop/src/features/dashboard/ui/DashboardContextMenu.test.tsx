// @vitest-environment happy-dom

import { fireEvent, render, screen } from '@testing-library/react';
import { type ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';

import { DashboardContextMenu } from './DashboardContextMenu';

const wrapper = ({ children }: { children: ReactNode }) => <TranslationProvider>{children}</TranslationProvider>;

type MenuProps = Omit<Parameters<typeof DashboardContextMenu>[0], 'children'>;

const renderMenu = (props: Partial<MenuProps> = {}, area?: ReactNode) =>
  render(
    <DashboardContextMenu showCleanup onAddWidget={vi.fn()} onCleanup={vi.fn()} {...props}>
      {area ?? <div data-testid="area">background</div>}
    </DashboardContextMenu>,
    { wrapper },
  );

const openMenu = () => {
  fireEvent.contextMenu(screen.getByTestId('area'));
};

describe('DashboardContextMenu', () => {
  it('shows both actions when showCleanup is true', () => {
    renderMenu();
    openMenu();

    expect(screen.getByText('Add Widget')).toBeInTheDocument();
    expect(screen.getByText('Cleanup Dashboard')).toBeInTheDocument();
  });

  it('hides Cleanup Dashboard when showCleanup is false', () => {
    renderMenu({ showCleanup: false });
    openMenu();

    expect(screen.getByText('Add Widget')).toBeInTheDocument();
    expect(screen.queryByText('Cleanup Dashboard')).not.toBeInTheDocument();
  });

  it('fires onAddWidget when Add widget is selected', () => {
    const onAddWidget = vi.fn();
    renderMenu({ onAddWidget });
    openMenu();

    fireEvent.click(screen.getByText('Add Widget'));

    expect(onAddWidget).toHaveBeenCalledTimes(1);
  });

  it('fires onCleanup when Cleanup Dashboard is selected', () => {
    const onCleanup = vi.fn();
    renderMenu({ onCleanup });
    openMenu();

    fireEvent.click(screen.getByText('Cleanup Dashboard'));

    expect(onCleanup).toHaveBeenCalledTimes(1);
  });

  it('does not open over a card (.react-grid-item)', () => {
    renderMenu(
      {},
      <div data-testid="area" className="react-grid-item">
        card
      </div>,
    );
    fireEvent.contextMenu(screen.getByTestId('area'));

    expect(screen.queryByText('Add Widget')).not.toBeInTheDocument();
  });
});
