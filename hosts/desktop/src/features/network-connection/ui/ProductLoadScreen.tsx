import { ProductLoadingScreen } from '@/shared/components';
import { useProductLoadPhase } from '../hooks/useProductLoadPhase';

import { ProductLoadNotice } from './ProductLoadNotice';

type Props = { identifier: string };

/**
 * Mounts with the load and unmounts with it, so the phase clock restarts on every
 * retry. A timed-out spinner stops animating: it would otherwise report progress
 * that is not happening.
 */
export const ProductLoadScreen = ({ identifier }: Props) => {
  const phase = useProductLoadPhase();

  return (
    <ProductLoadingScreen
      spinnerAnimated={phase !== 'timeout'}
      message={<ProductLoadNotice identifier={identifier} phase={phase} />}
    />
  );
};
