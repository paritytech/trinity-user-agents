// @vitest-environment happy-dom

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { TranslationProvider } from '@/shared/translation';
import { type Product } from '@/domains/product';
import { onProductRefreshRequestedSideEffect } from '@/aggregates/product-loading';

import { ProductWidgetBody } from './ProductWidgetBody';

// The refresh request is a DI side effect: a handler registered on it observes the
// real fan-out, so nothing has to stand in for the aggregate that declares it.
const onRefreshApplyMock = vi.fn();

// Tests are exempt from the no-`as` rule; only the fields the body reads matter.
const someProduct = { baseName: 'app.dot' } as unknown as Product;

type BodyProps = {
  product?: Product | null;
  hasContent?: boolean;
  pending?: boolean;
  onRemoveCard?: VoidFunction;
};

const renderBody = ({ product = null, hasContent = false, pending = false, onRemoveCard = vi.fn() }: BodyProps = {}) =>
  render(
    <TranslationProvider>
      <ProductWidgetBody
        productId="app.dot"
        product={product}
        hasContent={hasContent}
        pending={pending}
        onRemoveCard={onRemoveCard}
      />
    </TranslationProvider>,
  );

describe('ProductWidgetBody placeholders', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    onProductRefreshRequestedSideEffect.registerHandler({ available: () => true, body: onRefreshApplyMock });
  });

  afterEach(() => {
    onProductRefreshRequestedSideEffect.resetHandlers();
  });

  it('renders "widget not found" placeholder and removes card on action click', async () => {
    const user = userEvent.setup();
    const onRemoveCard = vi.fn();

    renderBody({ product: null, hasContent: false, pending: false, onRemoveCard });

    expect(screen.getByText('Widget does not exist anymore')).toBeTruthy();
    await user.click(screen.getByText('Delete widget'));
    expect(onRemoveCard).toHaveBeenCalledOnce();
  });

  it('renders "widget unavailable" placeholder and retries loading on action click', async () => {
    const user = userEvent.setup();

    renderBody({ product: someProduct, hasContent: false, pending: false });

    expect(screen.getByText('Widget is currently unavailable')).toBeTruthy();
    await user.click(screen.getByText('Retry'));
    expect(onRefreshApplyMock).toHaveBeenCalledWith({ identifier: 'app.dot' });
  });

  it('renders nothing (block pulse handles it) while phase-1 loading', () => {
    const { container } = renderBody({ product: someProduct, hasContent: false, pending: true });

    // Empty, not merely text-free: `WidgetLoadingScreen` is a bare pulse div, so a
    // textContent check would pass with it on screen.
    expect(container).toBeEmptyDOMElement();
  });
});
