package io.paritytech.polkadotapp.feature_products_api.domain.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.common.R as RCommon

/** Why a funding session ended without moving value. */
sealed interface FundingFailureReason {
    data object RegionUnavailable : FundingFailureReason

    data object VerificationRequired : FundingFailureReason

    data object VerificationRefused : FundingFailureReason

    data object BelowMinimum : FundingFailureReason

    data object AboveMaximum : FundingFailureReason

    data object InsufficientBalance : FundingFailureReason

    data object Expired : FundingFailureReason

    data object WrongAssetOrChain : FundingFailureReason

    data object ProviderTimeout : FundingFailureReason

    data object Cancelled : FundingFailureReason

    data object Refunded : FundingFailureReason

    data object Declined : FundingFailureReason

    /** A provider's own failure: its [code] and what it said. */
    data class Other(val code: String, val message: String) : FundingFailureReason
}

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
