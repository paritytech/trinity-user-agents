// @vitest-environment happy-dom

import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { type DashboardCard } from '@/domains/application';

import { DashboardCardChrome } from './DashboardCardChrome';

const card: DashboardCard = {
  i: 'c1',
  x: 2,
  y: 0,
  w: 1,
  h: 4,
  payload: { kind: 'product:widget', productId: 'app.dot' } as DashboardCard['payload'],
};

const renderChrome = (isLoading: boolean) =>
  render(
    <TranslationProvider>
      <DashboardCardChrome
        card={card}
        width={1}
        height={4}
        layoutRules={null}
        isLoading={isLoading}
        onResizeCard={() => {}}
        onRemoveCard={() => {}}
      >
        <div data-testid="body">content</div>
      </DashboardCardChrome>
    </TranslationProvider>,
  );

describe('DashboardCardChrome loading pulse', () => {
  it('pulses the block with a column-based delay while loading', () => {
    const { container } = renderChrome(true);
    const block = container.querySelector('.animate-widget-pulse');
    expect(block).toBeTruthy();
    // card.x = 2 → 2 * 200ms
    expect((block as HTMLElement).style.animationDelay).toBe('400ms');
  });

  it('does not pulse and fades the body in when loaded', () => {
    const { container } = renderChrome(false);
    expect(container.querySelector('.animate-widget-pulse')).toBeNull();
    expect(container.querySelector('.fade-in')).toBeTruthy();
  });
});
