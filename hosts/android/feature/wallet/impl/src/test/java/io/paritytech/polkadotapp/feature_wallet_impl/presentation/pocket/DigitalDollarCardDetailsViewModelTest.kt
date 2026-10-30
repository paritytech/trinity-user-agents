package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ShareCoinageLogsUseCase
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.DigitalDollarCardDetailsInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.OFFRAMP_URL
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.ONRAMP_URL
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.ParkedFundingDomainProvider
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetNeverOpened
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetOpenedOnce
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.verifySheetOpenedTwice
import io.paritytech.polkadotapp.test_shared.assertPresentationErrorShown
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class DigitalDollarCardDetailsViewModelTest {
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
    fun `a Get tap opens the top-up sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedOnce(ONRAMP_URL)
        router.verifySheetNeverOpened(OFFRAMP_URL)
    }

    @Test
    fun `a second Get tap while the funding config loads opens one sheet`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        viewModel.onGetCashClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedOnce(ONRAMP_URL)
    }

    @Test
    fun `after Get opened its sheet the next Get tap opens it again`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        fundingDomainProvider.completeReadsWithConfig()
        viewModel.onGetCashClick()
        fundingDomainProvider.completeReadsWithConfig()

        router.verifySheetOpenedTwice(ONRAMP_URL)
    }

    @Test
    fun `a failed funding config read shows the top-up error and unlocks Get`() = runBlocking<Unit> {
        val viewModel = createViewModel()

        viewModel.onGetCashClick()
        fundingDomainProvider.failReads()
        viewModel.onGetCashClick()
        fundingDomainProvider.completeReadsWithConfig()

        assertPresentationErrorShown<GetCashUnavailablePresentationError>(viewModel)
        router.verifySheetOpenedOnce(ONRAMP_URL)
    }

    private fun createViewModel() = DigitalDollarCardDetailsViewModel(
        interactor = interactor,
        router = router,
        tokenAmountMapper = tokenAmountMapper
    )
}
