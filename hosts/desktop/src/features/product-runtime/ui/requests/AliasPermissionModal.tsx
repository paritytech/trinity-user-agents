import { type AccountAliasReview } from '@parity/truapi-host';

import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { productRuntimeService } from '../../service';
import { RequestContextNote } from '../productRequestParts';

import { PermissionRequestModal } from './PermissionRequestModal';

type Props = {
  review: AccountAliasReview;
  onDecide: (approved: boolean) => void;
};

/**
 * RFC-0004 contextual alias: a stable pseudonym for this user inside another product's
 * context, which is what lets two products recognise the same person.
 *
 * The old modal offered "Allow once" beside "Always allow". The core answers a review
 * with a boolean and keeps the stored decision itself, so there is one Allow here.
 * TODO(truapi): restore the once/always split when `confirmUserAction` can carry it —
 * see docs/_plans/truapi-upstream-issues.md.
 */
export const AliasPermissionModal = ({ review, onDecide }: Props) => {
  const { t } = useTranslation();

  return (
    <PermissionRequestModal
      identifier={review.callingProductId}
      title={t('widget.productContainerBinding.aliasPermission.title', { requestedIdentifier: review.context.productId })}
      subtitle={t('widget.productContainerBinding.aliasPermission.subtitle')}
      note={<RequestContextNote context={productRuntimeService.formatProofContext(review.context)} />}
      warning={t('widget.productContainerBinding.aliasPermission.warning')}
      denyLabel={t('widget.productContainerBinding.aliasPermission.deny')}
      allowLabel={t('widget.productContainerBinding.aliasPermission.allowAlways')}
      dialogTestId={TEST_IDS.aliasPermissionDialog}
      allowTestId={TEST_IDS.aliasPermissionAllow}
      onAllow={() => onDecide(true)}
      onDeny={() => onDecide(false)}
    />
  );
};
