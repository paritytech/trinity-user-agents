package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.collections.immutable.ImmutableList

/** The CASH card's funding lists: what is in progress, then the history by day. */
data class FundingActivityUiState(
    val inFlight: ImmutableList<FundingActivityRowUi>,
    val days: ImmutableList<FundingActivityDayUi>,
)

data class FundingActivityDayUi(
    val title: FundingDayTitle,
    val rows: ImmutableList<FundingActivityRowUi>,
)

sealed interface FundingDayTitle {
    data object Today : FundingDayTitle

    data object Yesterday : FundingDayTitle

    data class Date(val text: String) : FundingDayTitle
}

data class FundingActivityRowUi(
    val id: String,
    val title: FundingRowTitle,
    val subtitle: FundingRowSubtitle,
    val icon: FundingRowIcon,
    val amount: TokenAmountModel?,
    val amountSign: String,
    val amountTone: FundingAmountTone,
)

enum class FundingRowTitle {
    TOPPING_UP,
    SENDING,
    TOPPED_UP,
    SENT,
    REFUNDED,
    PAYOUT_FAILED,
    TOP_UP_FAILED,
    WITHDRAW_FAILED,
}

sealed interface FundingRowSubtitle {
    data class Step(val step: FundingActivityStep) : FundingRowSubtitle

    data object Delayed : FundingRowSubtitle

    data class Ended(val rail: FundingRailType?, val moment: FundingMoment) : FundingRowSubtitle
}

/** "Today at 08:25", "Yesterday at 18:40", "4 Oct at 16:20", "4 Oct 2025". */
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
