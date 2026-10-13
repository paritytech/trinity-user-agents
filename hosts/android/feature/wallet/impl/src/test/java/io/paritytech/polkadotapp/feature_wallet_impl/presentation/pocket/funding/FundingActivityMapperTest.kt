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
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowIcon
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowSubtitle
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingRowTitle
import org.junit.Assert.assertEquals
import org.junit.Test
import java.time.ZoneOffset
import java.util.Locale
import kotlin.time.Instant

class FundingActivityMapperTest {
    private val asset = mockk<Chain.Asset> { every { precision } returns 6 }
    private val now = Instant.fromEpochMilliseconds(86_400_000)
    private val mapper = FundingActivityMapper(asset, "CASH", now, ZoneOffset.UTC, Locale.US)

    @Test
    fun `a failure's title names the direction it failed in`() {
        val failed = FundingActivityStatus.Failed(FundingFailureReason.Declined)

        val titles = mapper.map(history(item("in", FundingDirection.IN, failed), item("out", FundingDirection.OUT, failed)))
            .days.flatMap { it.rows }.map { it.title }

        assertEquals(listOf(FundingRowTitle.TOP_UP_FAILED, FundingRowTitle.WITHDRAWAL_FAILED), titles)
    }

    @Test
    fun `a refunded payout says what came back`() {
        val row = mapper.map(history(item("out", FundingDirection.OUT, FundingActivityStatus.PayoutFailed(45_000_000.intoBalance()))))
            .days.single().rows.single()

        assertEquals(FundingRowTitle.PAYOUT_FAILED, row.title)
        assertEquals(FundingRowSubtitle.Refunded("$45 CASH"), row.subtitle)
    }

    @Test
    fun `a session taking longer than quoted is flagged`() {
        val row = mapper.map(
            FundingActivity(inFlight = listOf(item("in", FundingDirection.IN, FundingActivityStatus.InProgress(FundingActivityStep.TakingLonger))), history = emptyList()),
        ).inFlight.single()

        assertEquals(FundingRowIcon.IN_PROGRESS_WARNING, row.icon)
        assertEquals(FundingRowSubtitle.Step(FundingActivityStep.TakingLonger), row.subtitle)
    }

    private fun history(vararg items: FundingActivityItem) = FundingActivity(inFlight = emptyList(), history = items.toList())

    private fun item(id: String, direction: FundingDirection, status: FundingActivityStatus) = FundingActivityItem(
        id = id,
        direction = direction,
        rail = FundingRailType.CARD,
        amount = 50_000_000.intoBalance(),
        date = now,
        status = status,
    )
}
