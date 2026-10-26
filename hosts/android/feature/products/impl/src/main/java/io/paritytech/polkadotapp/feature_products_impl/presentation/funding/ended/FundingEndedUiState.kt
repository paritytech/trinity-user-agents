package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.ended

import androidx.annotation.StringRes
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTimelineUi
import kotlinx.collections.immutable.ImmutableList
import kotlin.time.Instant

data class FundingEndedUiState(
    val succeeded: Boolean,
    val icon: FundingEndedIcon,
    val figure: String?,
    val figureTone: FundingEndedTone,
    val settledAt: Instant?,
    val timeline: FundingTimelineUi,
    val note: FundingEndedText?,
    val rows: ImmutableList<FundingEndedRow>,
    val action: FundingEndedAction,
)

enum class FundingEndedIcon {
    IN,
    OUT,
    FAILED,
}

enum class FundingEndedTone {
    SUCCESS,
    PRIMARY,
    TERTIARY,
}

enum class FundingEndedAction {
    CLOSE,
    START_OVER,
}

sealed interface FundingEndedText {
    data class Res(@param:StringRes val res: Int, val args: List<String>) : FundingEndedText

    data class Reason(val reason: FundingFailureReason) : FundingEndedText
}

sealed interface FundingEndedRow {
    data class Settled(@param:StringRes val caption: Int, val value: String, val requested: String?) : FundingEndedRow

    data class Value(@param:StringRes val caption: Int, val value: FundingEndedText) : FundingEndedRow

    data class Provider(val providerId: String) : FundingEndedRow

    data class Copy(@param:StringRes val caption: Int, val value: String) : FundingEndedRow

    data class Moment(@param:StringRes val caption: Int, val at: Instant) : FundingEndedRow
}
