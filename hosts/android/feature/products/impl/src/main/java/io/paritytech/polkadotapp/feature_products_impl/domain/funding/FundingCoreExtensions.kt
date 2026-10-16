package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.FundingStage

/** The order the rail tabs are drawn in. */
val FUNDING_RAIL_ORDER: List<FundingRail> = listOf(FundingRail.CRYPTO, FundingRail.CARD, FundingRail.BANK)

fun FundingCandidate.routes(rail: FundingRail, direction: FundingDirection): List<FundingRoute> =
    routes.filter { it.rail == rail && direction in it.directions }

fun FundingCandidate.serves(rail: FundingRail, direction: FundingDirection): Boolean =
    routes(rail, direction).isNotEmpty()

val FundingQuoteState.quote: FundingQuote?
    get() = (this as? FundingQuoteState.Quoted)?.quote

val FundingQuoteState.refusal: FundingQuoteRefusal?
    get() = ((this as? FundingQuoteState.Unavailable)?.reason as? FundingQuoteUnavailable.Refused)?.reason

val FundingQuoteState.isPending: Boolean
    get() = this is FundingQuoteState.Pending

val FundingStage.isOpen: Boolean
    get() = this is FundingStage.Open

/** Why the session ended, once it has failed. */
val FundingStage.failure: FundingFailure?
    get() = (this as? FundingStage.Failed)?.reason

fun FundingFailure.toReason(): FundingFailureReason = when (this) {
    FundingFailure.RegionUnavailable -> FundingFailureReason.RegionUnavailable
    FundingFailure.VerificationRequired -> FundingFailureReason.VerificationRequired
    FundingFailure.VerificationRefused -> FundingFailureReason.VerificationRefused
    FundingFailure.BelowMinimum -> FundingFailureReason.BelowMinimum
    FundingFailure.AboveMaximum -> FundingFailureReason.AboveMaximum
    FundingFailure.InsufficientBalance -> FundingFailureReason.InsufficientBalance
    FundingFailure.Expired -> FundingFailureReason.Expired
    FundingFailure.WrongAssetOrChain -> FundingFailureReason.WrongAssetOrChain
    FundingFailure.ProviderTimeout -> FundingFailureReason.ProviderTimeout
    FundingFailure.Cancelled -> FundingFailureReason.Cancelled
    FundingFailure.Refunded -> FundingFailureReason.Refunded
    FundingFailure.Declined -> FundingFailureReason.Declined
    is FundingFailure.Other -> FundingFailureReason.Other(code, message)
}
