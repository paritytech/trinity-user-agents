package io.paritytech.polkadotapp.app

import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService

@EntryPoint
@InstallIn(SingletonComponent::class)
interface IntegrationTestEntryPoint {
    fun remoteConfigService(): RemoteConfigService
    fun chainRegistry(): ChainRegistry
    fun chainConnectionRefCounter(): ChainConnectionRefCounter
    fun viewFunctionsApi(): MultiChainViewFunctionsApi
}
