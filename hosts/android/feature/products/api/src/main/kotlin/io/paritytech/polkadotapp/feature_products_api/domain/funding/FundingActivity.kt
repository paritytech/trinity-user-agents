package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import kotlin.time.Instant

/** How value moves between the user's balance and the outside. */
enum class FundingRailType {
    CARD,
    BANK,
    CRYPTO,
}

/** The CASH card's funding lists: sessions in flight, then ended ones, newest first. */
data class FundingActivity(
    val inFlight: List<FundingActivityItem>,
    val history: List<FundingActivityItem>,
)

/** One row of the funding lists. */
data class FundingActivityItem(
    val id: String,
    val direction: FundingDirection,
    val rail: FundingRailType?,
    val amount: Balance?,
    val date: Instant,
    val status: FundingActivityStatus,
)

sealed interface FundingActivityStatus {
    /** In flight, with where it has got to; [delayed] when slower than the provider said. */
    data class InProgress(val step: FundingActivityStep, val delayed: Boolean) : FundingActivityStatus

    data object ToppedUp : FundingActivityStatus

    data object Sent : FundingActivityStatus

    data object Refunded : FundingActivityStatus

    data object PayoutFailed : FundingActivityStatus

    data object Failed : FundingActivityStatus
}

/** Where a session in flight stands. */
sealed interface FundingActivityStep {
    data object Upcoming : FundingActivityStep

    data object WaitingForTransfer : FundingActivityStep

    data object Converting : FundingActivityStep

    data object Retrying : FundingActivityStep

    data object TransactionInitiated : FundingActivityStep

    data class ConvertingOut(val asset: String) : FundingActivityStep
}
