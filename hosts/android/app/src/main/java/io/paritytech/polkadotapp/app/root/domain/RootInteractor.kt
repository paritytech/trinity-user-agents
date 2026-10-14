package io.paritytech.polkadotapp.app.root.domain

import io.paritytech.polkadotapp.app.root.data.storage.OneShotTooltip
import io.paritytech.polkadotapp.app.root.data.storage.TooltipStorage
import io.paritytech.polkadotapp.app.root.domain.debug.VerifyUsernameOnChainUseCase
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.wrapIntoResult
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_account_api.data.repository.getCandidateAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.updaters.CandidateBalancesUpdateSystem
import io.paritytech.polkadotapp.feature_balances_api.data.updaters.WalletBalancesUpdateSystem
import io.paritytech.polkadotapp.feature_coinage_api.data.updaters.CoinageUpdateSystem
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.updaters.DotNsGatewayUpdateSystem
import io.paritytech.polkadotapp.feature_people_api.data.updaters.PeopleUpdateSystem
import io.paritytech.polkadotapp.feature_prices_api.domain.SyncPricesUseCase
import io.paritytech.polkadotapp.feature_splash_api.domain.DevResetCoordinator
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.merge
import timber.log.Timber
import javax.inject.Inject

interface RootInteractor {
    suspend fun syncPrices()

    fun startUpdateSystems(): Flow<*>

    suspend fun printAccountAddresses()

    suspend fun isDevResetNeeded(): Boolean

    suspend fun clearAllAndClose()

    fun shouldShowNetworkStatusTooltip(): Boolean

    fun markNetworkStatusTooltipShown()
}

class RealRootInteractor @Inject constructor(
    private val walletBalancesUpdateSystem: WalletBalancesUpdateSystem,
    private val candidateBalancesUpdateSystem: CandidateBalancesUpdateSystem,
    private val syncPricesUseCase: SyncPricesUseCase,
    private val peopleUpdateSystem: PeopleUpdateSystem,
    private val dotNsGatewayUpdateSystem: DotNsGatewayUpdateSystem,
    private val coinageUpdateSystem: CoinageUpdateSystem,
    private val chainRegistry: ChainRegistry,
    private val accountRepository: AccountRepository,
    private val verifyUsernameOnChainUseCase: VerifyUsernameOnChainUseCase,
    private val devResetCoordinator: DevResetCoordinator,
    private val tooltipStorage: TooltipStorage,
) : RootInteractor {
    override fun startUpdateSystems(): Flow<*> {
        val updateSystems = buildList {
            add(walletBalancesUpdateSystem.updateSystem.start())
            add(candidateBalancesUpdateSystem.updateSystem.start())
            add(dotNsGatewayUpdateSystem.updateSystem.start())
            add(coinageUpdateSystem.updateSystem.start())

            if (FeatureOption.PERSONHOOD.isEnabled) {
                add(peopleUpdateSystem.updateSystem.start())
            }
        }

        return updateSystems.merge().wrapIntoResult()
            .logFailure("Unexpected failure when syncing balances")
    }

    override suspend fun syncPrices() {
        syncPricesUseCase.syncPrices()
            .logFailure("Failed to sync prices")
    }

    override suspend fun printAccountAddresses() {
        val candidate = accountRepository.getCandidateAccount()
        printAccountAddress(candidate)

        val wallet = accountRepository.getWalletAccount()
        printAccountAddress(wallet)
    }

    override suspend fun isDevResetNeeded(): Boolean {
        if (!BuildConfig.DEBUG) return false
        if (!accountRepository.areAccountsInitialized()) return false
        return verifyUsernameOnChainUseCase().getOrNull() == false
    }

    override suspend fun clearAllAndClose() {
        devResetCoordinator.clearAllAndClose()
    }

    override fun shouldShowNetworkStatusTooltip(): Boolean = !tooltipStorage.wasShown(OneShotTooltip.NetworkStatus)

    override fun markNetworkStatusTooltipShown() {
        tooltipStorage.markShown(OneShotTooltip.NetworkStatus)
    }

    private suspend fun printAccountAddress(account: MetaAccount) {
        val address = account.addressIn(chainRegistry.peopleChain())
        Timber.w("Account ${account.name} has address $address in ${chainRegistry.peopleChain().name}")
    }
}
