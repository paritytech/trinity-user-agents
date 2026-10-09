package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModelEvent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

private const val EVENT_TIMEOUT_MS = 1_000L

@OptIn(ExperimentalCoroutinesApi::class)
class DigitalDollarCardDetailsViewModelTest {
    private val coinageHoldingsUseCase = mockk<CoinageHoldingsUseCase> {
        every { subscribeHoldings() } returns emptyFlow()
    }
    private val coinageBackupService = mockk<CoinageBackupService> {
        every { subscribeProgress() } returns flowOf(BackupProgress.NotStarted)
    }

    // Each open parks until the test answers it, as the overlay holds the call while it is up.
    private val pendingOpens = ArrayDeque<CompletableDeferred<Result<String?>>>()
    private val hostFunding = mockk<HostFunding> {
        coEvery { openFunding(any(), null) } coAnswers {
            CompletableDeferred<Result<String?>>().also(pendingOpens::addLast).await()
        }
        every { observeActivity() } returns emptyFlow()
    }

    private val interactor = DigitalDollarCardDetailsInteractor(
        chainAssetProvider = mockk(),
        environment = TestnetEnvironment.PRODUCTION,
        coinageTestnetFundUseCase = mockk(),
        coinageBackupService = coinageBackupService,
        shareCoinageLogsUseCase = mockk(),
        hostFunding = hostFunding,
        coinageHoldingsUseCase = coinageHoldingsUseCase,
        coinageBalanceConverterUseCase = mockk()
    )

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `+ opens inbound funding with no amount`() = runBlocking<Unit> {
        createViewModel().onGetCashClick()
        answerOpens(Result.success("intent"))

        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    // The overlay is up for as long as the call is, so a second tap would stack a second session behind it.
    @Test
    fun `a tap while the overlay is up opens nothing more`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        viewModel.onGetCashClick()
        answerOpens(Result.success(null))

        coVerify(exactly = 1) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    @Test
    fun `a dismissed overlay unlocks the button`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        answerOpens(Result.success(null))
        viewModel.onGetCashClick()
        answerOpens(Result.success(null))

        coVerify(exactly = 2) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    @Test
    fun `a failed open shows an error and unlocks the button`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        answerOpens(Result.failure(IllegalStateException("runtime unavailable")))
        viewModel.onGetCashClick()
        answerOpens(Result.success("intent"))

        assertPresentationErrorShown(viewModel)
        coVerify(exactly = 2) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    private fun answerOpens(result: Result<String?>) {
        while (pendingOpens.isNotEmpty()) {
            pendingOpens.removeFirst().complete(result)
        }
    }

    private suspend fun assertPresentationErrorShown(viewModel: DigitalDollarCardDetailsViewModel) {
        val event = withTimeout(EVENT_TIMEOUT_MS) { viewModel.events.first() }
        assertTrue("expected a PresentationError event but was $event", event is BaseViewModelEvent.PresentationError)
    }

    private fun createViewModel() = DigitalDollarCardDetailsViewModel(
        interactor = interactor,
        router = mockk(),
        tokenAmountMapper = mockk()
    )
}
