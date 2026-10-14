package io.paritytech.polkadotapp.feature_connection_status_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import dagger.multibindings.IntoSet
import io.paritytech.polkadotapp.feature_connection_status_api.domain.ChainHealthMonitor
import io.paritytech.polkadotapp.feature_connection_status_api.presentation.mixin.ChainHealthMixin
import io.paritytech.polkadotapp.feature_connection_status_impl.data.ChainAnchorDataSource
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchorSource
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.RealChainHealthMonitor
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe.BlockProductionProbe
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe.ChainHealthProbe
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe.UnansweredRequestProbe
import io.paritytech.polkadotapp.feature_connection_status_impl.presentation.mixin.RealChainHealthMixinFactory
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
interface ConnectionStatusFeatureModule {
    @Binds
    @Singleton
    fun bindChainHealthMonitor(impl: RealChainHealthMonitor): ChainHealthMonitor

    @Binds
    fun bindChainHealthMixinFactory(impl: RealChainHealthMixinFactory): ChainHealthMixin.Factory

    @Binds
    fun bindBlockProductionAnchorSource(impl: ChainAnchorDataSource): BlockProductionAnchorSource

    @Binds
    @IntoSet
    fun bindBlockProductionProbe(impl: BlockProductionProbe): ChainHealthProbe

    @Binds
    @IntoSet
    fun bindUnansweredRequestProbe(impl: UnansweredRequestProbe): ChainHealthProbe
}
