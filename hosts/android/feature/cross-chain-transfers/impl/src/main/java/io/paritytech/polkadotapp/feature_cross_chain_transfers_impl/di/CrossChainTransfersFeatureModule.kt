package io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.di

import dagger.Binds
import dagger.Module
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.feature_cross_chain_transfers_api.domain.CrossChainTransferService
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.CrossChainTransfersRepository
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.RealCrossChainTransfersRepository
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.RealXcmTransferDryRunner
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.data.dryRun.XcmTransferDryRunner
import io.paritytech.polkadotapp.feature_cross_chain_transfers_impl.domain.RealCrossChainTransferService

@Module
@InstallIn(SingletonComponent::class)
internal interface CrossChainTransfersFeatureApiModule {
    @Binds
    fun bindCrossChainTransfersRepository(real: RealCrossChainTransfersRepository): CrossChainTransfersRepository

    @Binds
    fun bindDryRunner(realXcmTransferDryRunner: RealXcmTransferDryRunner): XcmTransferDryRunner

    @Binds
    fun bindCrossChainTransferService(real: RealCrossChainTransferService): CrossChainTransferService
}
