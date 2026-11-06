package io.paritytech.polkadotapp.feature_products_api.presentation.funding

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.common.R as RCommon

/** Why the session ended, in a few words: the provider's own message when it gave one. */
val FundingFailureReason.reasonText: FundingReasonText
    get() = when (this) {
        FundingFailureReason.RegionUnavailable -> FundingReasonText.Res(RCommon.string.funding_failure_region_unavailable)
        FundingFailureReason.VerificationRequired -> FundingReasonText.Res(RCommon.string.funding_failure_verification_required)
        FundingFailureReason.VerificationRefused -> FundingReasonText.Res(RCommon.string.funding_failure_verification_refused)
        FundingFailureReason.BelowMinimum -> FundingReasonText.Res(RCommon.string.funding_failure_below_minimum)
        FundingFailureReason.AboveMaximum -> FundingReasonText.Res(RCommon.string.funding_failure_above_maximum)
        FundingFailureReason.InsufficientBalance -> FundingReasonText.Res(RCommon.string.funding_failure_insufficient_balance)
        FundingFailureReason.Expired -> FundingReasonText.Res(RCommon.string.funding_failure_expired)
        FundingFailureReason.WrongAssetOrChain -> FundingReasonText.Res(RCommon.string.funding_failure_wrong_asset_or_chain)
        FundingFailureReason.ProviderTimeout -> FundingReasonText.Res(RCommon.string.funding_failure_provider_timeout)
        FundingFailureReason.Cancelled -> FundingReasonText.Res(RCommon.string.funding_failure_cancelled)
        FundingFailureReason.Refunded -> FundingReasonText.Res(RCommon.string.funding_failure_refunded)
        FundingFailureReason.Declined -> FundingReasonText.Res(RCommon.string.funding_failure_declined)
        is FundingFailureReason.Other ->
            if (message.isBlank()) FundingReasonText.Res(RCommon.string.funding_failure_unknown) else FundingReasonText.Verbatim(message)
    }

/** A line of funding text: a string resource, or words a provider sent as they are. */
sealed interface FundingReasonText {
    data class Res(@param:StringRes val res: Int) : FundingReasonText

    data class Verbatim(val text: String) : FundingReasonText
}

@Composable
fun FundingReasonText.text(): String = when (this) {
    is FundingReasonText.Res -> stringResource(res)
    is FundingReasonText.Verbatim -> text
}

/** "Failed" and the reason, as a line under a session. */
@Composable
fun FundingFailureReason.failedText(): String = stringResource(RCommon.string.funding_failure_failed, reasonText.text())
