package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.FundingActivity
import uniffi.truapi.FundingActivityRow
import uniffi.truapi.FundingRail
import uniffi.truapi.FundingStatus
import java.math.BigInteger
import kotlin.time.Instant
import uniffi.truapi.FundingDirection as CoreFundingDirection

class TrUAPIHostFundingTest {
    private val activity = MutableStateFlow<FundingActivity?>(null)
    private val runtime = mockk<FundingRuntime> {
        every { activity } returns this@TrUAPIHostFundingTest.activity
        coEvery { start() } returns Result.success(Unit)
        coEvery { acknowledgeShown(any()) } returns Result.success(emptyList())
    }
    private val funding = TrUAPIHostFunding(
        runtime = runtime,
        sessionPresenter = mockk(),
    )

    @Test
    fun `opening passes the direction and amount to the core`() = runBlocking {
        coEvery { runtime.openFunding(CoreFundingDirection.OUT, "250") } returns Result.success("intent")

        assertEquals(Result.success("intent"), funding.openFunding(FundingDirection.OUT, Balance(BigInteger("250"))))
    }

    @Test
    fun `a dismissed overlay opens no session`() = runBlocking {
        coEvery { runtime.openFunding(CoreFundingDirection.IN, null) } returns Result.success(null)

        assertEquals(Result.success(null), funding.openFunding(FundingDirection.IN, null))
    }

    // The core merges, sorts and hides the rows; the app reads each one into the card's lists.
    @Test
    fun `the activity is the core's rows, read once the core started`() = runBlocking {
        activity.value = FundingActivity(
            inFlight = listOf(row("a", FundingStatus.WaitingForTransfer)),
            history = listOf(row("b", FundingStatus.ToppedUp)),
        )

        val read = funding.observeActivity().first()

        coVerify { runtime.start() }
        assertEquals(
            listOf(item("a", FundingActivityStatus.InProgress(FundingActivityStep.WaitingForTransfer))),
            read.inFlight,
        )
        assertEquals(listOf(item("b", FundingActivityStatus.ToppedUp)), read.history)
    }

    @Test
    fun `the sessions the history shows are acknowledged, and none when it shows none`() = runBlocking {
        funding.acknowledgeShown(listOf("b"))
        funding.acknowledgeShown(emptyList())

        coVerify(exactly = 1) { runtime.acknowledgeShown(any()) }
        coVerify { runtime.acknowledgeShown(listOf("b")) }
    }

    private fun row(intent: String, status: FundingStatus) = FundingActivityRow(
        intent = intent,
        direction = CoreFundingDirection.IN,
        rail = FundingRail.BANK,
        amount = "50000000",
        atMs = 1_000uL,
        status = status,
    )

    private fun item(id: String, status: FundingActivityStatus) = FundingActivityItem(
        id = id,
        direction = FundingDirection.IN,
        rail = FundingRailType.BANK,
        amount = Balance(BigInteger("50000000")),
        date = Instant.fromEpochMilliseconds(1_000),
        status = status,
    )
}
