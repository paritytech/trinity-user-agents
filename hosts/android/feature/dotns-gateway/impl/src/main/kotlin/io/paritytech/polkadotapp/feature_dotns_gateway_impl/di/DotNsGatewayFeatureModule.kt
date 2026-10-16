package io.paritytech.polkadotapp.feature_dotns_gateway_impl.di

import dagger.Binds
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.network.updaters.Updater
import io.paritytech.polkadotapp.chains.network.updaters.system.UpdateSystemFactory
import io.paritytech.polkadotapp.chains.storage.StorageCache
import io.paritytech.polkadotapp.feature_account_api.data.WalletAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.repository.DotNsGatewayRepository
import io.paritytech.polkadotapp.feature_dotns_gateway_api.data.updaters.DotNsGatewayUpdateSystem
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.DotNsGatewayConfigProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.config.RemoteConfigDotNsGatewayConfigProvider
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.repository.RealDotNsGatewayRepository
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.DotNsGatewayOrigins
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.signer.origins.RealDotNsGatewayOrigins
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.updater.AccountAliasUpdater
import io.paritytech.polkadotapp.feature_dotns_gateway_impl.data.updater.AccountNamesUpdater

@Module
@InstallIn(SingletonComponent::class)
internal interface DotNsGatewayFeatureModule {
    companion object {
        @Provides
        fun provideAccountAliasUpdater(
            @WalletAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            chainRegistry: ChainRegistry,
            storageCache: StorageCache
        ): AccountAliasUpdater {
            return AccountAliasUpdater(
                chainRegistry,
                storageCache,
                accountUpdateScope
            )
        }

        @Provides
        fun provideAccountNamesUpdater(
            @WalletAccount accountUpdateScope: Updater.NoChainScope<MetaAccount>,
            chainRegistry: ChainRegistry,
            storageCache: StorageCache
        ): AccountNamesUpdater {
            return AccountNamesUpdater(
                chainRegistry,
                storageCache,
                accountUpdateScope
            )
        }

        @Provides
        fun provideDotNsGatewayUpdateSystem(
            knownChains: KnownChains,
            updateSystemFactory: UpdateSystemFactory,
            accountAliasUpdater: AccountAliasUpdater,
            accountNamesUpdater: AccountNamesUpdater
        ): DotNsGatewayUpdateSystem {
            val updateSystem = updateSystemFactory.createConstantSingleChain(
                listOf(accountAliasUpdater, accountNamesUpdater),
                knownChains.assetHub
            )
            return DotNsGatewayUpdateSystem(updateSystem)
        }
    }

    @Binds
    fun bindDotNsGatewayOrigins(impl: RealDotNsGatewayOrigins): DotNsGatewayOrigins

    @Binds
    fun bindDotNsGatewayRepository(impl: RealDotNsGatewayRepository): DotNsGatewayRepository

    @Binds
    fun bindDotNsGatewayConfigProvider(impl: RemoteConfigDotNsGatewayConfigProvider): DotNsGatewayConfigProvider
}
