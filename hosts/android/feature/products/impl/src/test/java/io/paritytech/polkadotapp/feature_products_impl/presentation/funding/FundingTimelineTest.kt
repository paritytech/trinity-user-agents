package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.FundingBand
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingProgressStep
import uniffi.truapi.FundingStep
import io.paritytech.polkadotapp.common.R as RCommon

class FundingTimelineTest {
    @Test
    fun `a started top-up waiting for payment sits on the first dot after Started`() {
        val timeline = FundingTimeline.live(steps(started = 1uL), FundingBand.AWAITING_PAYMENT, FundingDirection.IN, failed = false)

        assertEquals(
            listOf(DONE, CURRENT, PENDING, PENDING, PENDING, PENDING, PENDING),
            timeline.dots,
        )
        assertEquals(listOf(RCommon.string.funding_step_started, RCommon.string.funding_step_received, RCommon.string.funding_step_added), timeline.labels)
    }

    @Test
    fun `checking the payment moves to the second dot`() {
        val timeline = FundingTimeline.live(steps(started = 1uL), FundingBand.CHECKING_PAYMENT, FundingDirection.IN, failed = false)

        assertEquals(CURRENT, timeline.dots[2])
    }

    @Test
    fun `once received the band places the dot past the middle milestone`() {
        val timeline = FundingTimeline.live(steps(started = 1uL, received = 2uL), FundingBand.ADDING, FundingDirection.IN, failed = false)

        assertEquals(listOf(DONE, DONE, DONE, DONE, DONE, CURRENT, PENDING), timeline.dots)
    }

    @Test
    fun `a withdrawal ends on Sent, and every dot is done once it is reached`() {
        val timeline = FundingTimeline.live(steps(started = 1uL, received = 2uL, last = 3uL), FundingBand.SENT, FundingDirection.OUT, failed = false)

        assertEquals(List(FundingTimeline.DOTS) { DONE }, timeline.dots)
        assertEquals(RCommon.string.funding_step_sent, timeline.labels.last())
    }

    @Test
    fun `a failure is red where the session stood, and only a milestone's label turns red`() {
        val between = FundingTimeline.live(steps(started = 1uL), FundingBand.ENDED, FundingDirection.IN, failed = true)
        val onMilestone = FundingTimeline.at(FundingTimeline.MILESTONE_GAP, failed = true, labels = FundingTimeline.labels(FundingDirection.IN))

        assertEquals(FAILED, between.dots[1])
        assertNull(between.failedLabel)
        assertEquals(1, onMilestone.failedLabel)
    }

    private fun steps(started: ULong?, received: ULong? = null, last: ULong? = null) = listOf(
        FundingProgressStep(FundingStep.STARTED, started),
        FundingProgressStep(FundingStep.RECEIVED, received),
        FundingProgressStep(FundingStep.ADDED, last),
    )

    private companion object {
        val DONE = FundingDotState.DONE
        val CURRENT = FundingDotState.CURRENT
        val PENDING = FundingDotState.PENDING
        val FAILED = FundingDotState.FAILED
    }
}
