package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModelEvent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ShareCoinageLogsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
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

private const val ONRAMP_URL = "onramp.example"
private const val OFFRAMP_URL = "offramp.example"
private const val EVENT_TIMEOUT_MS = 1_000L

@OptIn(ExperimentalCoroutinesApi::class)
class DigitalDollarCardDetailsViewModelTest {
    private val fundingConfig = FundingConfig(onrampUrl = ONRAMP_URL, offrampUrl = OFFRAMP_URL)

    private val chainAssetProvider: ChainAssetProvider = mockk()
    private val coinageTestnetFundUseCase: CoinageTestnetFundUseCase = mockk()
    private val coinageBackupService: CoinageBackupService = mockk()
    private val shareCoinageLogsUseCase: ShareCoinageLogsUseCase = mockk()
    private val coinageHoldingsUseCase: CoinageHoldingsUseCase = mockk()
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase = mockk()
    private val router: PocketRouter = mockk(relaxUnitFun = true)
    private val tokenAmountMapper: TokenAmountMapper = mockk()

    private val fundingDomainProvider = ParkedFundingDomainProvider()

    private val interactor = DigitalDollarCardDetailsInteractor(
        chainAssetProvider = chainAssetProvider,
        environment = TestnetEnvironment.PRODUCTION,
        coinageTestnetFundUseCase = coinageTestnetFundUseCase,
        coinageBackupService = coinageBackupService,
        shareCoinageLogsUseCase = shareCoinageLogsUseCase,
        fundingDomainProvider = fundingDomainProvider,
        coinageHoldingsUseCase = coinageHoldingsUseCase,
        coinageBalanceConverterUseCase = coinageBalanceConverterUseCase
    )

    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        every { coinageHoldingsUseCase.subscribeHoldings() } returns emptyFlow()
        every { coinageBackupService.subscribeProgress() } returns flowOf(BackupProgress.NotStarted)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `a second + tap while the funding config loads opens one sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        viewModel.onGetCashClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(ONRAMP_URL)
    }

    @Test
    fun `a Withdraw tap while + loads opens only the + sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        viewModel.onWithdrawClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(ONRAMP_URL)
        verifySheetNeverOpened(OFFRAMP_URL)
    }

    @Test
    fun `a second Withdraw tap while the funding config loads opens one sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onWithdrawClick()
        viewModel.onWithdrawClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(OFFRAMP_URL)
    }

    @Test
    fun `a + tap while Withdraw loads opens only the Withdraw sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onWithdrawClick()
        viewModel.onGetCashClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(OFFRAMP_URL)
        verifySheetNeverOpened(ONRAMP_URL)
    }

    @Test
    fun `after + opened its sheet a Withdraw tap opens the Withdraw sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        withFundingConfigLoaded()
        viewModel.onWithdrawClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(ONRAMP_URL)
        verifySheetOpenedOnce(OFFRAMP_URL)
    }

    @Test
    fun `after Withdraw opened its sheet a + tap opens the + sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onWithdrawClick()
        withFundingConfigLoaded()
        viewModel.onGetCashClick()
        withFundingConfigLoaded()

        verifySheetOpenedOnce(OFFRAMP_URL)
        verifySheetOpenedOnce(ONRAMP_URL)
    }

    @Test
    fun `a failed funding config read unlocks the buttons`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        withFundingConfigFailing()
        viewModel.onWithdrawClick()
        withFundingConfigLoaded()

        assertPresentationErrorShown(viewModel)
        verifySheetOpenedOnce(OFFRAMP_URL)
    }

    private fun withFundingConfigLoaded() {
        fundingDomainProvider.completeReads(Result.success(fundingConfig))
    }

    private fun withFundingConfigFailing() {
        fundingDomainProvider.completeReads(Result.failure(IllegalStateException("no config")))
    }

    private fun verifySheetOpenedOnce(url: String) {
        verify(exactly = 1) { router.openSpaSheet(url) }
    }

    private fun verifySheetNeverOpened(url: String) {
        verify(exactly = 0) { router.openSpaSheet(url) }
    }

    private suspend fun assertPresentationErrorShown(viewModel: DigitalDollarCardDetailsViewModel) {
        val event = withTimeout(EVENT_TIMEOUT_MS) { viewModel.events.first() }
        assertTrue("expected a PresentationError event but was $event", event is BaseViewModelEvent.PresentationError)
    }

    private fun createViewModel() = DigitalDollarCardDetailsViewModel(
        interactor = interactor,
        router = router,
        tokenAmountMapper = tokenAmountMapper
    )

    private class ParkedFundingDomainProvider : FundingDomainProvider {
        private val reads = ArrayDeque<CompletableDeferred<Result<FundingConfig>>>()

        override suspend fun getFundingConfig(): Result<FundingConfig> {
            val read = CompletableDeferred<Result<FundingConfig>>()
            reads.addLast(read)
            return read.await()
        }

        override suspend fun getFundingProductIds(): Result<Set<ProductId>> = error("not reachable from the card")

        fun completeReads(result: Result<FundingConfig>) {
            while (reads.isNotEmpty()) {
                reads.removeFirst().complete(result)
            }
        }
    }
}
