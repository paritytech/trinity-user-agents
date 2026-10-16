package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingDirection
import io.paritytech.polkadotapp.feature_products_api.domain.funding.HostFunding
import io.paritytech.polkadotapp.feature_wallet_impl.domain.funding.FundingEntryPoints
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.BalanceRestoreUiState
import io.paritytech.polkadotapp.test_shared.assertPresentationErrorShown
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
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
        coEvery { openFunding(any(), null) } coAnswers {
            CompletableDeferred<Result<String?>>().also(pendingOpens::addLast).await()
        }
        every { observeActivity() } returns emptyFlow()
    }

    private var fundingVisible = true
    private val fundingEntryPoints = mockk<FundingEntryPoints> { every { visible } answers { fundingVisible } }

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

        assertPresentationErrorShown<GetCashUnavailablePresentationError>(viewModel)
        coVerify(exactly = 2) { hostFunding.openFunding(FundingDirection.IN, null) }
    }

    /** A build that cannot run a funding provider, such as release, offers Send but not Get. */
    @Test
    fun `get is offered only where funding is`() = runBlocking<Unit> {
        every { coinageBackupService.subscribeProgress() } returns flowOf(BackupProgress.NotStarted)

        fundingVisible = true
        assertEquals(BalanceRestoreUiState.SendCash(canGetCash = true), balanceRestore(createViewModel()))

        fundingVisible = false
        assertEquals(BalanceRestoreUiState.SendCash(canGetCash = false), balanceRestore(createViewModel()))
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
        tokenAmountMapper = mockk(),
        paymentAssetBrandProvider = mockk { every { brand } returns MutableStateFlow(PaymentAssetBrand.mocked) },
        fundingEntryPoints = fundingEntryPoints,
    )
}
