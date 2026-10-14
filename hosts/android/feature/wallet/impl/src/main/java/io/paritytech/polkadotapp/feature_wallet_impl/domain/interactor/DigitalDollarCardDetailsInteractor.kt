package io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageBackupService
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldings
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageTestnetFundUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.ShareCoinageLogsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.FundingConfig
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.CoinageHoldingsInfo
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.toHoldingsInfo
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

class DigitalDollarCardDetailsInteractor @Inject constructor(
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider,
    private val environment: TestnetEnvironment,
    private val coinageTestnetFundUseCase: CoinageTestnetFundUseCase,
    private val coinageBackupService: CoinageBackupService,
    private val shareCoinageLogsUseCase: ShareCoinageLogsUseCase,
    private val fundingDomainProvider: FundingDomainProvider,
    private val coinageHoldingsUseCase: CoinageHoldingsUseCase,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase
) {
    companion object {
        private val TOP_UP_AMOUNT = 150.toBigDecimal()
        private val NIGHTLY_TOP_UP_AMOUNT = 10.toBigDecimal()
    }

    suspend fun getFundingConfig(): Result<FundingConfig> = fundingDomainProvider.getFundingConfig()

    suspend fun asset(): Chain.Asset = chainAssetProvider.asset()

    /**
     * The four figures and the rows beneath them, off one classification.
     *
     * Deliberately not two streams: the composition bar is a picture of the same numbers printed above it,
     * so reading them a tick apart is the one way they could ever disagree.
     */
    fun observeHoldings(): Flow<Result<CoinageHoldingsInfo>> =
        coinageHoldingsUseCase.subscribeHoldings()
            .map { holdingsInfoOf(it) }
            .logFailure("DigitalDollarCardDetailsInteractor: Failed to classify coinage holdings")

    fun observeActionsEnabled(): Flow<Boolean> = coinageBackupService.subscribeProgress()
        .map { it.actionsEnabled() }

    fun observeBackupProgress(): Flow<BackupProgress> = coinageBackupService.subscribeProgress()

    context(scope: ComputationalScope)
    fun startDeepSearch() = coinageBackupService.deepSearch()

    context(scope: ComputationalScope)
    fun markBackupCompleted() = coinageBackupService.markAsCompleted()

    suspend fun shareCoinageLogs(): Result<Unit> = shareCoinageLogsUseCase().map { }

    suspend fun autoFundAvailable() = environment != TestnetEnvironment.PRODUCTION

    suspend fun testnetFund(): Result<Unit> {
        val amount = when (environment) {
            TestnetEnvironment.TESTNET -> TOP_UP_AMOUNT
            TestnetEnvironment.NIGHTLY, TestnetEnvironment.PRODUCTION -> NIGHTLY_TOP_UP_AMOUNT
        }

        return coinageTestnetFundUseCase(amount)
    }

    private fun BackupProgress.actionsEnabled() = this !is BackupProgress.Initial && this !is BackupProgress.Deep

    private suspend fun holdingsInfoOf(holdings: CoinageHoldings): Result<CoinageHoldingsInfo> =
        coinageBalanceConverterUseCase.create().map { conversion ->
            with(conversion) { holdings.toHoldingsInfo() }
        }
}
