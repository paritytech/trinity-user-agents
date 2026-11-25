import { type Product, useProductIcon } from '@/domains/product';

export type ProductHeaderViewModel = {
  name: string;
  description?: string;
  iconSrc?: string;
};

// Builds the presentation view-model for a product header: maps the semantic
// `Product` (displayName / baseName / icon) to the rendered `name` / `description`
// / `iconSrc`. The `baseName → description` choice is a presentation decision, so
// it lives here (a widget) rather than in the product domain.
export function useProductHeaderProps(options: {
  product: Nullable<Product>;
  fallbackName?: string;
  fallbackDomain?: string;
}): ProductHeaderViewModel {
  const { product, fallbackName = '', fallbackDomain = fallbackName } = options;
  const { data: iconUrl } = useProductIcon(product?.icon ?? null);
  const name = product?.displayName ?? fallbackName;
  const domain = product?.baseName ?? fallbackDomain;

  return {
    name,
    // `ProductHeader` renders `description` only when it's non-empty and differs
    // from `name`, so no need to pre-filter the equal/empty case here.
    description: domain,
    iconSrc: iconUrl ?? undefined,
  };
}
