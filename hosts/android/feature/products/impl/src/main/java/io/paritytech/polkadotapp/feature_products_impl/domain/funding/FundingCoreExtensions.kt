package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingCandidate
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingMode
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteRefusal
import uniffi.truapi.FundingQuoteState
import uniffi.truapi.FundingQuoteUnavailable
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingRoute
import uniffi.truapi.FundingStage
import uniffi.truapi.RouteDirection

/** The order the rail tabs are drawn in. */
val FUNDING_RAIL_ORDER: List<FundingRail> = listOf(FundingRail.CRYPTO, FundingRail.CARD, FundingRail.BANK)

val FundingRail.mode: FundingMode
    get() = when (this) {
        FundingRail.CARD -> FundingMode.CARD
        FundingRail.BANK -> FundingMode.BANK
        FundingRail.CRYPTO -> FundingMode.CRYPTO
    }

val FundingDirection.routeDirection: RouteDirection
    get() = when (this) {
        FundingDirection.IN -> RouteDirection.IN
        FundingDirection.OUT -> RouteDirection.OUT
    }

fun FundingCandidate.routes(rail: FundingRail, direction: FundingDirection): List<FundingRoute> =
    routes.filter { it.mode == rail.mode && direction.routeDirection in it.directions }

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
