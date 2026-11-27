package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingActivity
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.funding.RealFundingEntriesMixin
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
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
import org.junit.Assert.assertEquals
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

    private val interactor = DigitalDollarCardDetailsInteractor(
        chainAssetProvider = chainAssetProvider,
        environment = TestnetEnvironment.PRODUCTION,
        coinageTestnetFundUseCase = mockk(),
        coinageBackupService = coinageBackupService,
        shareCoinageLogsUseCase = mockk(),
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

    // Every build offers Get and Send, as on iOS; with no provider listed, Get says funding is unavailable.
    @Test
    fun `get and send are offered in every build`() = runBlocking<Unit> {
        every { coinageBackupService.subscribeProgress() } returns flowOf(BackupProgress.NotStarted)

        assertEquals(BalanceRestoreUiState.SendCash, balanceRestore(createViewModel()))
    }

    private suspend fun balanceRestore(viewModel: DigitalDollarCardDetailsViewModel): BalanceRestoreUiState =
        withTimeout(EVENT_TIMEOUT_MS) {
            viewModel.state.first { it.balanceRestore != BalanceRestoreUiState.NotDetermined }.balanceRestore
        }

    private fun answerOpens(result: Result<String?>) {
        while (pendingOpens.isNotEmpty()) {
            pendingOpens.removeFirst().complete(result)
        }
    }

    private fun createViewModel() = DigitalDollarCardDetailsViewModel(
        interactor = interactor,
        router = mockk(),
        tokenAmountMapper = mockk(relaxed = true),
        fundingEntriesMixinFactory = RealFundingEntriesMixin.Factory(hostFunding, chainAssetProvider, mockk(relaxed = true), FakeTimeProvider { 0L }),
    )
}
