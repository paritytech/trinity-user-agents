package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityItem
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStatus
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivityStep
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingRailType
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.GetCashUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.WithdrawUnavailablePresentationError
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.FundingSheet
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Instant

private const val EVENT_TIMEOUT_MS = 1_000L

@OptIn(ExperimentalCoroutinesApi::class)
class FundingActivityMixinTest {
    // Each open parks until the test answers it, as the overlay holds the call while it is up.
    private val pendingOpens = ArrayDeque<CompletableDeferred<Result<String?>>>()
    private val hostFunding = mockk<HostFunding> {
        coEvery { isFundingAvailable() } returns Result.success(true)
        coEvery { openFunding(any(), null) } coAnswers {
            CompletableDeferred<Result<String?>>().also(pendingOpens::addLast).await()
        }
        every { observeActivity() } returns flowOf(FundingActivity(inFlight = emptyList(), history = emptyList()))
        coEvery { acknowledgeShown(any()) } returns Unit
    }
    private val chainAssetProvider = mockk<ChainAssetProvider> {
        coEvery { asset() } returns mockk { every { precision } returns 6 }
    }
    private val scope = CoroutineScope(SupervisorJob() + UnconfinedTestDispatcher())
    private val failures = mutableListOf<PresentationThrowable>()

    @After
    fun tearDown() = scope.cancel()

    @Test
    fun `get and send open funding their way with no amount`() = runBlocking<Unit> {
        val mixin = mixin()

        mixin.onStart(FundingDirection.IN)
        answerOpens(Result.success("in"))
        mixin.onStart(FundingDirection.OUT)
        answerOpens(Result.success("out"))

        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.IN, null) }
        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.OUT, null) }
    }

    // The overlay is up for as long as the call is, so a second tap would stack a second session behind it.
    @Test
    fun `a tap while the overlay is up opens nothing more, and a dismissed one unlocks it`() = runBlocking<Unit> {
        val mixin = mixin()

        mixin.onStart(FundingDirection.IN)
        mixin.onStart(FundingDirection.IN)
        answerOpens(Result.success(null))
        mixin.onStart(FundingDirection.IN)
        answerOpens(Result.success(null))

        coVerify(exactly = 2) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    @Test
    fun `a failed open says which way is unavailable and unlocks the button`() = runBlocking<Unit> {
        val mixin = mixin()

        mixin.onStart(FundingDirection.IN)
        answerOpens(Result.failure(IllegalStateException("runtime unavailable")))
        mixin.onStart(FundingDirection.OUT)
        answerOpens(Result.failure(IllegalStateException("runtime unavailable")))

        assertTrue(failures[0] is GetCashUnavailablePresentationError)
        assertTrue(failures[1] is WithdrawUnavailablePresentationError)
        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.OUT, null) }
    }

    @Test
    fun `with no provider listed, funding says it is unavailable and opens nothing`() = runBlocking<Unit> {
        coEvery { hostFunding.isFundingAvailable() } returns Result.success(false)
        val mixin = mixin()

        mixin.onStart(FundingDirection.IN)
        mixin.onStart(FundingDirection.OUT)

        assertTrue(failures[0] is GetCashUnavailablePresentationError)
        assertTrue(failures[1] is WithdrawUnavailablePresentationError)
        coVerify(exactly = 0) { hostFunding.openFunding(any(), any()) }
    }

    // A second top-up while one is unpaid is how a user pays twice, so the running ones show first.
    @Test
    fun `sessions in progress show first, and starting a new one opens funding`() = runBlocking<Unit> {
        every { hostFunding.observeActivity() } returns flowOf(FundingActivity(inFlight = listOf(inProgress(FundingDirection.IN)), history = emptyList()))
        val mixin = mixin()
        withTimeout(EVENT_TIMEOUT_MS) { mixin.activity.first { it.toppingUp.isNotEmpty() } }

        mixin.onStart(FundingDirection.IN)
        assertEquals(FundingSheet.TOPPING_UP, mixin.sheet.value)
        coVerify(exactly = 0) { hostFunding.openFunding(any(), any()) }

        mixin.onHistory()
        assertEquals(FundingSheet.HISTORY, mixin.sheet.value)
        mixin.onSheetBack()
        assertEquals(FundingSheet.TOPPING_UP, mixin.sheet.value)

        mixin.onStartNew(FundingDirection.IN)
        answerOpens(Result.success("intent"))

        assertNull(mixin.sheet.value)
        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    @Test
    fun `a withdrawal in progress shows first on send, not on get`() = runBlocking<Unit> {
        every { hostFunding.observeActivity() } returns flowOf(FundingActivity(inFlight = listOf(inProgress(FundingDirection.OUT)), history = emptyList()))
        val mixin = mixin()
        withTimeout(EVENT_TIMEOUT_MS) { mixin.activity.first { it.sending.isNotEmpty() } }

        mixin.onStart(FundingDirection.OUT)
        assertEquals(FundingSheet.SENDING, mixin.sheet.value)
        mixin.onSheetDismiss()
        mixin.onStart(FundingDirection.IN)
        answerOpens(Result.success("intent"))

        assertNull(mixin.sheet.value)
        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    // The core keeps an ended session owed to the host until the user has seen it, which is only in the history.
    @Test
    fun `ended sessions are acknowledged only while the history shows them`() = runBlocking<Unit> {
        val ended = inProgress(FundingDirection.IN).copy(id = "ended", status = FundingActivityStatus.ToppedUp)
        every { hostFunding.observeActivity() } returns flowOf(FundingActivity(inFlight = listOf(inProgress(FundingDirection.IN)), history = listOf(ended)))
        val mixin = mixin()
        withTimeout(EVENT_TIMEOUT_MS) { mixin.activity.first { it.completed.isNotEmpty() } }

        mixin.onStart(FundingDirection.IN)
        coVerify(exactly = 0) { hostFunding.acknowledgeShown(any()) }

        mixin.onHistory()
        coVerify(timeout = EVENT_TIMEOUT_MS) { hostFunding.acknowledgeShown(listOf("ended")) }
    }

    @Test
    fun `a tap before the first list arrives waits for it, and shows the sessions in progress`() = runBlocking<Unit> {
        val activity = MutableSharedFlow<FundingActivity>()
        every { hostFunding.observeActivity() } returns activity
        val mixin = mixin()

        mixin.onStart(FundingDirection.IN)
        assertNull(mixin.sheet.value)
        activity.emit(FundingActivity(inFlight = listOf(inProgress(FundingDirection.IN)), history = emptyList()))

        assertEquals(FundingSheet.TOPPING_UP, mixin.sheet.value)
        coVerify(exactly = 0) { hostFunding.openFunding(any(), any()) }
    }

    @Test
    fun `the history as a screen of its own acknowledges its ended sessions without opening a sheet`() = runBlocking<Unit> {
        val ended = inProgress(FundingDirection.IN).copy(id = "ended", status = FundingActivityStatus.ToppedUp)
        every { hostFunding.observeActivity() } returns flowOf(FundingActivity(inFlight = emptyList(), history = listOf(ended)))
        val mixin = mixin()

        mixin.onHistoryShown()

        coVerify(timeout = EVENT_TIMEOUT_MS) { hostFunding.acknowledgeShown(listOf("ended")) }
        assertNull(mixin.sheet.value)
    }

    private fun mixin(): FundingActivityMixin {
        val mixin = RealFundingActivityMixin.Factory(hostFunding, chainAssetProvider, mockk(relaxed = true), FakeTimeProvider { 0L })
            .create(scope) { failures += it }
        // The activity is shared while a screen reads it.
        mixin.activity.launchIn(scope)
        return mixin
    }

    private fun inProgress(direction: FundingDirection) = FundingActivityItem(
        id = if (direction == FundingDirection.IN) "in" else "out",
        direction = direction,
        rail = FundingRailType.BANK,
        amount = 50_000_000.intoBalance(),
        date = Instant.fromEpochMilliseconds(0),
        status = FundingActivityStatus.InProgress(FundingActivityStep.WaitingForPayment),
    )

    private fun answerOpens(result: Result<String?>) {
        while (pendingOpens.isNotEmpty()) {
            pendingOpens.removeFirst().complete(result)
        }
    }
}
