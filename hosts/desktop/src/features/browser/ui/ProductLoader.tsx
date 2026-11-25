import { ProductLoadingScreen } from '@/shared/components';
import { useTransformer } from '@/shared/di';
import { resolveProductLoaderTransformer } from '../di';

type Props = { identifier: string };

/** Mounted only while a product surface is loading — the phase clock starts here. */
export const ProductLoader = ({ identifier }: Props) => {
  const resolved = useTransformer(resolveProductLoaderTransformer, { identifier });

  return resolved ?? <ProductLoadingScreen identifier={identifier} />;
};
