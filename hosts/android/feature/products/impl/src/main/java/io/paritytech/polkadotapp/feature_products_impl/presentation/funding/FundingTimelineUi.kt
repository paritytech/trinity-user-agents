package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import androidx.annotation.StringRes
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgressStep
import uniffi.truapi.FundingStep
import io.paritytech.polkadotapp.common.R as RCommon

// Seven dots: three milestones, with two between each pair for the band.
data class FundingTimelineUi(
    val dots: ImmutableList<FundingDotState>,
    @param:StringRes val labels: ImmutableList<Int>,
    val failedLabel: Int?,
)

enum class FundingDotState {
    DONE,
    CURRENT,
    PENDING,
    FAILED,
}

object FundingTimeline {
    const val DOTS = 7
    const val MILESTONE_GAP = 3
    private const val MILESTONES = 3

    fun at(position: Int, failed: Boolean, labels: List<Int>): FundingTimelineUi = FundingTimelineUi(
        dots = List(DOTS) { index ->
            when {
                index < position -> FundingDotState.DONE
                index > position -> FundingDotState.PENDING
                failed -> FundingDotState.FAILED
                else -> FundingDotState.CURRENT
            }
        }.toImmutableList(),
        labels = labels.toImmutableList(),
        failedLabel = position.takeIf { failed && it % MILESTONE_GAP == 0 }?.div(MILESTONE_GAP),
    )

    fun labels(direction: FundingDirection): List<Int> = listOf(
        RCommon.string.funding_step_started,
        RCommon.string.funding_step_received,
        if (direction == FundingDirection.IN) RCommon.string.funding_step_added else RCommon.string.funding_step_sent,
    )

    fun live(steps: List<FundingProgressStep>, band: FundingBand, direction: FundingDirection, failed: Boolean): FundingTimelineUi {
        val reached = steps.filter { it.reachedAtMs != null }.maxOfOrNull { it.step.milestone() + 1 } ?: 0
        val position = when (reached) {
            0 -> 0
            MILESTONES -> DOTS
            else -> (reached - 1) * MILESTONE_GAP + band.between()
        }
        return at(position, failed, labels(direction))
    }

    fun FundingStep.milestone(): Int = when (this) {
        FundingStep.STARTED -> 0
        FundingStep.RECEIVED -> 1
        FundingStep.ADDED, FundingStep.SENT -> 2
    }

    private fun FundingBand.between(): Int = when (this) {
        FundingBand.CHECKING_PAYMENT, FundingBand.ADDING, FundingBand.SENDING -> 2
        else -> 1
    }
}
