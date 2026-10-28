package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import kotlinx.collections.immutable.ImmutableList

/** The Purse's funding lists: what is in progress, delayed first, then what completed, newest first. */
data class FundingActivityUiState(
    val inFlight: ImmutableList<FundingActivityRowUi>,
    val completed: ImmutableList<FundingActivityRowUi>,
    val currentBalance: String?,
) {
    /** Top-ups in progress, for the Topping up sheet. */
    val toppingUp: List<FundingActivityRowUi>
        get() = inFlight.filter { it.inbound }

    /** Withdrawals in progress, for the Sending sheet. */
    val sending: List<FundingActivityRowUi>
        get() = inFlight.filterNot { it.inbound }
}

/** The funding sheets: the sessions in progress one way, then the history of both. */
enum class FundingSheet {
    TOPPING_UP,
    SENDING,
    HISTORY,
}

/** One row: the amount as its title, the rail in a pill, and a line under it. */
data class FundingActivityRowUi(
    val id: String,
    val inbound: Boolean,
    /** "$50 CASH" in flight in, "+$50 CASH" once added, "-$50 CASH" out, unsigned once nothing moved. */
    val amount: String?,
    val amountTone: FundingAmountTone,
    val rail: FundingRailType?,
    val subtitle: FundingRowSubtitle,
    val icon: FundingRowIcon,
)

sealed interface FundingRowSubtitle {
    /** In flight: where the session stands. */
    data class Step(val step: FundingActivityStep, val inbound: Boolean) : FundingRowSubtitle

    /** Ended: "{moment}" when the money moved, otherwise what happened before it. */
    data class Ended(val outcome: FundingRowOutcome, val moment: FundingMoment) : FundingRowSubtitle
}

enum class FundingRowOutcome {
    MOVED,
    REFUNDED,
    PAYOUT_FAILED,
    EXPIRED,
    PAYMENT_FAILED,
    CANCELLED,
}

/** "Today at 12:45", "Yesterday at 12:45", "Jun 12 at 16:40", "Jun 12, 2025". */
sealed interface FundingMoment {
    data class TodayAt(val time: String) : FundingMoment

    data class YesterdayAt(val time: String) : FundingMoment

    data class DayAt(val day: String, val time: String) : FundingMoment

    data class Date(val text: String) : FundingMoment
}

enum class FundingRowIcon {
    IN_PROGRESS,
    IN_PROGRESS_WARNING,
    MOVED_IN,
    MOVED_OUT,
    FAILED,
}

enum class FundingAmountTone {
    SUCCESS,
    PRIMARY,
    SECONDARY,
}
