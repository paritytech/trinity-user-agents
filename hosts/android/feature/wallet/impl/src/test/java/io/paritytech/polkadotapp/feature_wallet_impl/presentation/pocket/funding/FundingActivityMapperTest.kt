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
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingMoment
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowOutcome
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import org.junit.Assert.assertEquals
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
    fun `a failure reads why and when, under an unsigned amount`() {
        val failed = FundingActivityStatus.Failed(FundingFailureReason.Declined)

        val row = mapper.map(history(item("in", FundingDirection.IN, failed)), currentBalance = null).completed.single()

        assertEquals(FundingRowOutcome.PAYMENT_FAILED, (row.subtitle as FundingRowSubtitle.Ended).outcome)
        assertEquals("$50 CASH", row.amount)
        assertEquals(FundingAmountTone.SECONDARY, row.amountTone)
        assertEquals(FundingRowIcon.FAILED, row.icon)
    }

    @Test
    fun `a failed payout and a cancel read their outcome, with no refund line`() {
        val rows = mapper.map(
            history(
                item("payout", FundingDirection.OUT, FundingActivityStatus.PayoutFailed(45_000_000.intoBalance())),
                item("cancel", FundingDirection.OUT, FundingActivityStatus.Cancelled(45_000_000.intoBalance())),
            ),
            currentBalance = null,
        ).completed

        assertEquals(
            listOf(FundingRowOutcome.PAYOUT_FAILED, FundingRowOutcome.CANCELLED),
            rows.map { (it.subtitle as FundingRowSubtitle.Ended).outcome },
        )
        assertEquals(listOf("$50 CASH", "$50 CASH"), rows.map { it.amount })
        assertEquals(listOf(FundingRailType.CARD, FundingRailType.CARD), rows.map { it.rail })
    }

    @Test
    fun `signs and tones follow which way the money moved`() {
        val rows = mapper.map(
            history(item("in", FundingDirection.IN, FundingActivityStatus.ToppedUp), item("out", FundingDirection.OUT, FundingActivityStatus.Sent)),
            currentBalance = null,
        ).completed

        assertEquals(listOf("+$50 CASH", "-$50 CASH"), rows.map { it.amount })
        assertEquals(listOf(FundingAmountTone.SUCCESS, FundingAmountTone.PRIMARY), rows.map { it.amountTone })
        assertEquals(listOf(FundingRowIcon.MOVED_IN, FundingRowIcon.MOVED_OUT), rows.map { it.icon })
    }

    @Test
    fun `moments read today, yesterday, then the day, and the date in another year`() {
        val day = 86_400_000L
        val moments = mapper.map(
            history(
                item("today", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now),
                item("yesterday", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now - day.milliseconds),
                item("older", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = now - (3 * day).milliseconds),
            ),
            currentBalance = 200_000_000.intoBalance(),
        ).completed.map { (it.subtitle as FundingRowSubtitle.Ended).moment }

        assertEquals(listOf(FundingMoment.TodayAt::class, FundingMoment.YesterdayAt::class, FundingMoment.Date::class), moments.map { it::class })
        assertEquals(FundingMoment.Date("Dec 30, 1969"), moments.last())
    }

    @Test
    fun `top-ups and withdrawals in progress split for their sheets, an outbound one signed`() {
        val state = mapper.map(
            FundingActivity(
                inFlight = listOf(
                    item("in", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.WaitingForPayment)),
                    item("out", FundingDirection.OUT, FundingActivityStatus.InProgress(FundingActivityStep.Converting)),
                ),
                history = emptyList(),
            ),
            currentBalance = null,
        )

        assertEquals(listOf("in"), state.toppingUp.map { it.id })
        assertEquals(listOf("out"), state.sending.map { it.id })
        assertEquals(listOf("$50 CASH", "-$50 CASH"), state.inFlight.map { it.amount })
    }

    @Test
    fun `in progress shows a delayed session first, then the newest`() {
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
        assertEquals(FundingRowSubtitle.Step(refused, inbound = false), row.subtitle)
    }

    @Test
    fun `a session taking longer than quoted is flagged`() {
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("in", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.TakingLonger))), history = emptyList()),
            currentBalance = null,
        ).inFlight.single()

        assertEquals(FundingRowIcon.IN_PROGRESS_WARNING, row.icon)
        assertEquals(FundingRowSubtitle.Step(FundingActivityStep.TakingLonger, inbound = true), row.subtitle)
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
