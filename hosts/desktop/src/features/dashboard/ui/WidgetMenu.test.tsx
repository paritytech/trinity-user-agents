// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';

import { WidgetMenu } from './WidgetMenu';

const renderMenu = (menuItems?: React.ReactNode) =>
  render(
    <TranslationProvider>
      <WidgetMenu
        sizes={[]}
        currentSize={{ w: 1, h: 4 }}
        removeLabel="Remove Widget"
        menuItems={menuItems}
        isOpen
        onResize={vi.fn()}
        onRemove={vi.fn()}
        onOpenChange={vi.fn()}
      />
    </TranslationProvider>,
  );

describe('WidgetMenu', () => {
  it('renders contributed menuItems above the destructive Remove item', () => {
    renderMenu(<div data-testid="reload-item">Reload widget</div>);

    const reloadItem = screen.getByTestId('reload-item');
    const removeItem = screen.getByText('Remove Widget');

    expect(reloadItem).toBeTruthy();
    // menuItems must precede Remove in DOM order (Figma: …divider → Reload → Remove).
    expect(reloadItem.compareDocumentPosition(removeItem) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('omits the menuItems region when no items are contributed', () => {
    renderMenu();

    expect(screen.queryByTestId('reload-item')).toBeNull();
    expect(screen.getByText('Remove Widget')).toBeTruthy();
  });
});
