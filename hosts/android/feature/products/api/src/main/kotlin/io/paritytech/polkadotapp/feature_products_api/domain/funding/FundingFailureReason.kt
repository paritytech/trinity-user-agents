package io.paritytech.polkadotapp.feature_products_api.domain.funding

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.common.R as RCommon

/** Why a funding session ended without moving value, by a stable [code] a stored one reads back as. */
sealed class FundingFailureReason(val code: String) {
    data object RegionUnavailable : FundingFailureReason("regionUnavailable")

    data object VerificationRequired : FundingFailureReason("verificationRequired")

    data object VerificationRefused : FundingFailureReason("verificationRefused")

    data object BelowMinimum : FundingFailureReason("belowMinimum")

    data object AboveMaximum : FundingFailureReason("aboveMaximum")

    data object InsufficientBalance : FundingFailureReason("insufficientBalance")

    data object Expired : FundingFailureReason("expired")

    data object WrongAssetOrChain : FundingFailureReason("wrongAssetOrChain")

    data object ProviderTimeout : FundingFailureReason("providerTimeout")

    data object Cancelled : FundingFailureReason("cancelled")

    data object Refunded : FundingFailureReason("refunded")

    data object Declined : FundingFailureReason("declined")

    /** A provider's own failure, with what it said. */
    data class Other(val otherCode: String, val message: String) : FundingFailureReason(otherCode)

    companion object {
        private val NAMED by lazy {
            listOf(
                RegionUnavailable, VerificationRequired, VerificationRefused, BelowMinimum, AboveMaximum,
                InsufficientBalance, Expired, WrongAssetOrChain, ProviderTimeout, Cancelled, Refunded, Declined,
            )
        }

        /** The failure a stored [code] names, with the [message] an [Other] failure was stored with. */
        fun of(code: String, message: String?): FundingFailureReason =
            NAMED.firstOrNull { it.code == code } ?: Other(code, message.orEmpty())
    }
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
