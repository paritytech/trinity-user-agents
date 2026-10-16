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
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingStage
import uniffi.truapi.FundingUpdate

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

/**
 * Whether money moved for the session: the provider started or named funds, or saw the user's payment. The core's rule
 * is the same; from then on only the provider ends the session.
 */
val FundingSession.moneyMoved: Boolean
    get() = started.isNotEmpty() || updates.any { it.update.movesMoney }

private val FundingUpdate.movesMoney: Boolean
    get() = when (this) {
        is FundingUpdate.PaymentReceived,
        is FundingUpdate.Crediting,
        is FundingUpdate.Collecting,
        is FundingUpdate.Refunding,
        -> true
        else -> false
    }

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
