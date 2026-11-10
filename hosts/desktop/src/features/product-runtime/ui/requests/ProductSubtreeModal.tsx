import { type ProductSubtreeReview } from '@parity/truapi-host';

import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';

import { PermissionRequestModal } from './PermissionRequestModal';

type Props = {
  review: ProductSubtreeReview;
  onDecide: (approved: boolean) => void;
};

/**
 * A product resolving its account subtree, when the value is not cached and the core must
 * ask the paired device over SSO. It grants nothing lasting — one round-trip to the phone
 * — so there is no warning line, only the product asking and an allow/deny. Dismissing the
 * dialog denies (fail-closed), which the core maps to a rejected account request.
 */
export const ProductSubtreeModal = ({ review, onDecide }: Props) => {
  const { t } = useTranslation();

  return (
    <PermissionRequestModal
      identifier={review.productId}
      title={t('feature.productRuntime.review.productSubtree.title')}
      subtitle={t('feature.productRuntime.review.productSubtree.subtitle')}
      denyLabel={t('feature.productRuntime.review.deny')}
      allowLabel={t('common.action.allow')}
      dialogTestId={TEST_IDS.productSubtreeDialog}
      allowTestId={TEST_IDS.productSubtreeAllow}
      onAllow={() => onDecide(true)}
      onDeny={() => onDecide(false)}
    />
  );
};
