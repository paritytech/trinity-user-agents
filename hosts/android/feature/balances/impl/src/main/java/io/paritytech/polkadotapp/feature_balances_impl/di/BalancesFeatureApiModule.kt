package io.paritytech.polkadotapp.feature_balances_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_account_api.data.CandidateAccount
import io.paritytech.polkadotapp.feature_account_api.data.WalletAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.repository.BalanceRepository
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.updaters.CandidateBalancesUpdateSystem
import io.paritytech.polkadotapp.feature_balances_api.data.updaters.WalletBalancesUpdateSystem
import io.paritytech.polkadotapp.feature_balances_impl.data.repository.RealBalanceRepository
import io.paritytech.polkadotapp.feature_balances_impl.data.type.RealTokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_impl.data.updaters.BalancesUpdateSystem
import io.paritytech.polkadotapp.feature_balances_impl.data.updaters.BalancesUpdater
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
internal interface BalancesFeatureApiModule {
    @Binds
    fun bindTokenBalanceTypeRegistry(impl: RealTokenBalanceTypeRegistry): TokenBalanceTypeRegistry

    @Binds
    fun bindBalanceRepository(impl: RealBalanceRepository): BalanceRepository

    companion object {
        @Provides
        @Singleton
        @WalletAccount
        fun provideWalletBalancesUpdater(
            @WalletAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            tokenBalanceTypeRegistry: TokenBalanceTypeRegistry
        ): BalancesUpdater {
            return BalancesUpdater(
                scope = accountUpdateScope,
                balanceTypeRegistry = tokenBalanceTypeRegistry
            )
        }

        @Provides
        @Singleton
        @CandidateAccount
        fun provideCandidateBalancesUpdater(
            @CandidateAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            tokenBalanceTypeRegistry: TokenBalanceTypeRegistry
        ): BalancesUpdater {
            return BalancesUpdater(
                scope = accountUpdateScope,
                balanceTypeRegistry = tokenBalanceTypeRegistry
            )
        }

        @Provides
        @Singleton
        fun provideWalletBalancesUpdateSystem(
            chainRegistry: ChainRegistry,
            @WalletAccount balancesUpdater: BalancesUpdater,
            @WalletAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
            coroutineDispatchers: CoroutineDispatchers,
        ): WalletBalancesUpdateSystem {
            val updateSystem = BalancesUpdateSystem(
                chainRegistry = chainRegistry,
                accountBalancesUpdater = balancesUpdater,
                accountUpdateScope = accountUpdateScope,
                storageSharedRequestsBuilderFactory = storageSharedRequestsBuilderFactory,
                coroutineDispatchers = coroutineDispatchers
            )

            return WalletBalancesUpdateSystem(updateSystem)
        }

        @Provides
        @Singleton
        fun provideCandidateBalancesUpdateSystem(
            chainRegistry: ChainRegistry,
            @CandidateAccount balancesUpdater: BalancesUpdater,
            @CandidateAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            storageSharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
            coroutineDispatchers: CoroutineDispatchers,
        ): CandidateBalancesUpdateSystem {
            val updateSystem = BalancesUpdateSystem(
                chainRegistry = chainRegistry,
                accountBalancesUpdater = balancesUpdater,
                accountUpdateScope = accountUpdateScope,
                storageSharedRequestsBuilderFactory = storageSharedRequestsBuilderFactory,
                coroutineDispatchers = coroutineDispatchers
            )

            return CandidateBalancesUpdateSystem(updateSystem)
        }
    }
}
