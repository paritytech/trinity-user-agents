package io.paritytech.polkadotapp.feature_products_api.domain.funding

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
