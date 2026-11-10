import { type CreateProofReview } from '@parity/truapi-host';

import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { productRuntimeService } from '../../service';
import { RequestContextNote } from '../productRequestParts';

import { PermissionRequestModal } from './PermissionRequestModal';

type Props = {
  review: CreateProofReview;
  onDecide: (approved: boolean) => void;
};

/** RFC-0024 proof of personhood: a ring signature that says "a human, not which one". */
export const ProofPermissionModal = ({ review, onDecide }: Props) => {
  const { t } = useTranslation();

  return (
    <PermissionRequestModal
      identifier={review.callingProductId}
      title={t('widget.productContainerBinding.proofPermission.title', { requestedIdentifier: review.context.productId })}
      subtitle={t('widget.productContainerBinding.proofPermission.subtitle')}
      note={<RequestContextNote context={productRuntimeService.formatProofContext(review.context)} />}
      warning={t('widget.productContainerBinding.proofPermission.warning')}
      denyLabel={t('widget.productContainerBinding.proofPermission.deny')}
      allowLabel={t('widget.productContainerBinding.proofPermission.allow')}
      dialogTestId={TEST_IDS.proofPermissionDialog}
      allowTestId={TEST_IDS.proofPermissionAllow}
      onAllow={() => onDecide(true)}
      onDeny={() => onDecide(false)}
    />
  );
};
