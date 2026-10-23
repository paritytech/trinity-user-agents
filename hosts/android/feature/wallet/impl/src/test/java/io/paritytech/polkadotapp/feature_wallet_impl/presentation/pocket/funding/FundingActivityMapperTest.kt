package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingDayTitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.ZoneOffset
import java.util.Locale
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Instant

class FundingActivityMapperTest {
    private val asset = mockk<Chain.Asset> { every { precision } returns 6 }
    private val now = Instant.fromEpochMilliseconds(86_400_000)
    private val mapper = FundingActivityMapper(asset, "CASH", now, ZoneOffset.UTC, Locale.US)

    @Test
    fun `a failure's title names the direction it failed in`() {
        val failed = FundingActivityStatus.Failed(FundingFailureReason.Declined)

        val titles = mapper.map(history(item("in", FundingDirection.IN, failed), item("out", FundingDirection.OUT, failed)), currentBalance = null)
            .days.flatMap { it.rows }.map { it.title }

        assertEquals(listOf(FundingRowTitle.TOP_UP_FAILED, FundingRowTitle.WITHDRAWAL_FAILED), titles)
    }

    @Test
    fun `a failed payout reads its rail and moment, with no refund line`() {
        val row = mapper.map(
            history(item("out", FundingDirection.OUT, FundingActivityStatus.PayoutFailed(45_000_000.intoBalance()))),
            currentBalance = null,
        ).days.single().rows.single()

        assertEquals(FundingRowTitle.PAYOUT_FAILED, row.title)
        assertEquals(FundingRailType.CARD, (row.subtitle as FundingRowSubtitle.Ended).rail)
        assertTrue(row.subtitle.moment is FundingMoment.TodayAt)
        assertEquals("$50 CASH", row.amount)
        assertEquals(FundingAmountTone.SECONDARY, row.amountTone)
    }

    @Test
    fun `a cancelled withdrawal reads its rail and moment, with no sign on the amount`() {
        val row = mapper.map(
            history(item("out", FundingDirection.OUT, FundingActivityStatus.Cancelled(45_000_000.intoBalance()))),
            currentBalance = null,
        ).days.single().rows.single()

        assertEquals(FundingRowTitle.CANCELLED, row.title)
        assertEquals(FundingRailType.CARD, (row.subtitle as FundingRowSubtitle.Ended).rail)
        assertTrue(row.subtitle.moment is FundingMoment.TodayAt)
        assertEquals(FundingRowIcon.FAILED, row.icon)
        assertEquals("$50 CASH", row.amount)
    }

    @Test
    fun `signs and tones follow which way the money moved`() {
        val rows = mapper.map(
            history(item("in", FundingDirection.IN, FundingActivityStatus.ToppedUp), item("out", FundingDirection.OUT, FundingActivityStatus.Sent)),
            currentBalance = null,
        ).days.single().rows

        assertEquals(listOf("+$50 CASH", "-$50 CASH"), rows.map { it.amount })
        assertEquals(listOf(FundingAmountTone.SUCCESS, FundingAmountTone.PRIMARY), rows.map { it.amountTone })
        assertEquals(listOf(FundingRowIcon.MOVED_IN, FundingRowIcon.MOVED_OUT), rows.map { it.icon })
    }

    @Test
    fun `days are titled today, yesterday, then weekday and date`() {
        val day = 86_400_000L
        val titles = mapper.map(
            history(
                item("today", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now),
                item("yesterday", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now - day.milliseconds),
                item("older", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now - (3 * day).milliseconds),
            ),
            currentBalance = null,
        ).days.map { it.title }

        assertEquals(
            listOf(FundingDayTitle.Today, FundingDayTitle.Yesterday, FundingDayTitle.Date("Tuesday 30 December 1969")),
            titles,
        )
    }

    // Each day closes on the current balance less what every later day moved.
    @Test
    fun `each day carries the balance at its end`() {
        val state = mapper.map(
            history(
                item("in", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now),
                item("out", FundingDirection.OUT, FundingActivityStatus.Sent, at = now - 86_400_000L.milliseconds),
                item("older", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now - (2 * 86_400_000L).milliseconds),
            ),
            currentBalance = 200_000_000.intoBalance(),
        )

        assertEquals("$200 CASH", state.currentBalance)
        assertEquals(listOf("$200 CASH", "$150 CASH", "$200 CASH"), state.days.map { it.endOfDayBalance })
    }

    @Test
    fun `the dock shows a delayed session first, then the newest`() {
        val inFlight = mapper.map(
            FundingActivity(
                inFlight = listOf(
                    item("old", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.WaitingForPayment), at = now - 1000L.milliseconds),
                    item("new", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.WaitingForPayment), at = now),
                    item("slow", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.Retrying), at = now - 5000L.milliseconds),
                ),
                history = emptyList(),
            ),
            currentBalance = null,
        ).inFlight

        assertEquals(listOf("slow", "new", "old"), inFlight.map { it.id })
    }

    // A refusal leaves the session running on the provider's terms, which the user should notice.
    @Test
    fun `a refused cancel is flagged in flight`() {
        val refused = FundingActivityStep.CancelRefused("already converted")
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("out", FundingDirection.OUT, FundingActivityStatus.InProgress(refused))), history = emptyList()),
            currentBalance = null,
        ).inFlight.single()

        assertEquals(FundingRowIcon.IN_PROGRESS_WARNING, row.icon)
        assertEquals(FundingRowSubtitle.Step(refused), row.subtitle)
    }

    @Test
    fun `a session taking longer than quoted is flagged`() {
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("in", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.TakingLonger))), history = emptyList()),
            currentBalance = null,
        ).inFlight.single()

        assertEquals(FundingRowIcon.IN_PROGRESS_WARNING, row.icon)
        assertEquals(FundingRowSubtitle.Step(FundingActivityStep.TakingLonger), row.subtitle)
    }

    private fun history(vararg items: FundingActivityItem) = FundingActivity(inFlight = emptyList(), history = items.toList())

    private fun item(id: String, direction: FundingDirection, status: FundingActivityStatus, at: Instant = now) = FundingActivityItem(
        id = id,
        direction = direction,
        rail = FundingRailType.CARD,
        amount = 50_000_000.intoBalance(),
        date = at,
        status = status,
    )
}
