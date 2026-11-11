package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingFailureReason
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingActivityRowUi
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingAmountTone
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowOutcome
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test

class FundingActivityMapperTest {
    private val asset = mockk<Chain.Asset> { every { precision } returns 6 }
    private val now = Instant.fromEpochMilliseconds(86_400_000)
    private val mapper = FundingActivityMapper(asset, PlainAmounts)

    @Test
    fun `a failure reads why and when, under an unsigned amount`() {
        val failed = FundingActivityStatus.Failed(FundingFailureReason.Declined)

        val row = mapper.map(history(item("in", FundingDirection.IN, failed))).completed.single()

        assertEquals(FundingRowOutcome.PAYMENT_FAILED, (row.subtitle as FundingRowSubtitle.Ended).outcome)
        assertEquals("50", label(row))
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
        ).completed

        assertEquals(
            listOf(FundingRowOutcome.PAYOUT_FAILED, FundingRowOutcome.CANCELLED),
            rows.map { (it.subtitle as FundingRowSubtitle.Ended).outcome },
        )
        assertEquals(listOf("50", "50"), rows.map(::label))
        assertEquals(listOf(FundingRailType.CARD, FundingRailType.CARD), rows.map { it.rail })
    }

    @Test
    fun `signs and tones follow which way the money moved`() {
        val rows = mapper.map(
            history(item("in", FundingDirection.IN, FundingActivityStatus.ToppedUp), item("out", FundingDirection.OUT, FundingActivityStatus.Sent)),
        ).completed

        assertEquals(listOf("+50", "-50"), rows.map(::label))
        assertEquals(listOf(FundingAmountTone.SUCCESS, FundingAmountTone.PRIMARY), rows.map { it.amountTone })
        assertEquals(listOf(FundingRowIcon.MOVED_IN, FundingRowIcon.MOVED_OUT), rows.map { it.icon })
    }

    @Test
    fun `an ended row carries when it ended, for the row to format`() {
        val at = now - 86_400_000L.milliseconds
        val subtitle = mapper.map(history(item("done", FundingDirection.IN, FundingActivityStatus.ToppedUp, at = at))).completed.single().subtitle

        assertEquals(FundingRowSubtitle.Ended(FundingRowOutcome.MOVED, at), subtitle)
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
        )

        assertEquals(listOf("in"), state.toppingUp.map { it.id })
        assertEquals(listOf("out"), state.sending.map { it.id })
        assertEquals(listOf("50", "-50"), state.inFlight.map(::label))
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
        ).inFlight

        assertEquals(listOf("slow", "new", "old"), inFlight.map { it.id })
    }

    // A refusal leaves the session running on the provider's terms, which the user should notice.
    @Test
    fun `a refused cancel is flagged in flight`() {
        val refused = FundingActivityStep.CancelRefused("already converted")
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("out", FundingDirection.OUT, FundingActivityStatus.InProgress(refused))), history = emptyList()),
        ).inFlight.single()

        assertEquals(FundingRowIcon.IN_PROGRESS_WARNING, row.icon)
        assertEquals(FundingRowSubtitle.Step(refused, inbound = false), row.subtitle)
    }

    @Test
    fun `a session taking longer than quoted is flagged`() {
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("in", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.TakingLonger))), history = emptyList()),
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

    /** The sign and the amount in whole CASH, as the row formats them. */
    private fun label(row: FundingActivityRowUi): String? = row.amount?.let { it.sign + it.value.amount.stripTrailingZeros().toPlainString() }

    /** Models an amount by its value alone; the formatter the rows share is not under test here. */
    private object PlainAmounts : TokenAmountMapper {
        override fun mapFrom(source: ChainAssetWithAmount): TokenAmountModel = object : TokenAmountModel {
            override val amount = source.chainAsset.amountFromPlanks(source.amount)
            override val appearance = TokenSymbolAppearance.DigitalDollar
        }
    }
}
