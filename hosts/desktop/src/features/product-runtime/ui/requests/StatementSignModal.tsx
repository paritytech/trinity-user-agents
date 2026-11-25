import { type StatementStoreProductSignReview } from '@parity/truapi-host';

import { TEST_IDS } from '@/shared/test-ids';
import { useTranslation } from '@/shared/translation';
import { useProductAccountAddress } from '@/aggregates/truapi-runtime';
import { productRuntimeService } from '../../service';
import {
  RequestAccountDetailsSection,
  RequestDetailSection,
  RequestMonoLine,
  RequestSummaryNote,
  RequestSummaryRow,
} from '../productRequestParts';

import { ReviewRequestLayout } from './ReviewRequestLayout';

type Props = {
  review: StatementStoreProductSignReview;
  onDecide: (approved: boolean) => void;
};

/**
 * A statement-store payload signed by a product account.
 *
 * Deliberately not folded into `SignRawModal`: the payload is the exact unsigned
 * statement and is signed as-is, with no `<Bytes>` envelope, so presenting it with the
 * raw-signing convention would tell the user something untrue about what they approve.
 *
 * New with the core — the host-container binding submitted statements without a review —
 * so the copy is new rather than restored.
 */
export const StatementSignModal = ({ review, onDecide }: Props) => {
  const { t } = useTranslation();
  const payloadHex = productRuntimeService.formatTranscriptBytes(review.payload);
  const { address, pending: addressPending } = useProductAccountAddress(review.account);
  const addressFailed = !addressPending && address === null;

  return (
    <ReviewRequestLayout
      identifier={review.account.dotNsIdentifier}
      title={t('feature.productRuntime.review.statementSign.title')}
      primaryLabel={t('feature.browser.continueToSign')}
      primaryTestId={TEST_IDS.signReviewContinueButton}
      summary={
        <>
          <RequestSummaryNote tone="info">{t('feature.productRuntime.review.statementSign.subtitle')}</RequestSummaryNote>
          <RequestSummaryRow
            label={t('feature.browser.account')}
            value={productRuntimeService.formatProductAccount(review.account)}
            testId={TEST_IDS.signReviewAccount}
          />
        </>
      }
      details={
        <>
          <RequestAccountDetailsSection label={t('feature.browser.account')} address={address} failed={addressFailed} />
          <RequestDetailSection
            label={t('common.label.message')}
            copyValue={payloadHex}
            copyLabel={t('feature.browser.copyMessage')}
          >
            <RequestMonoLine>{payloadHex}</RequestMonoLine>
          </RequestDetailSection>
        </>
      }
      onApprove={() => onDecide(true)}
      onDeny={() => onDecide(false)}
    />
  );
};
