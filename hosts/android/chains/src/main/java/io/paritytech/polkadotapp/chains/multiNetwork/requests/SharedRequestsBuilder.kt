package io.paritytech.polkadotapp.chains.multiNetwork.requests

import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.paritytech.polkadotapp.chains.storage.StorageChange
import kotlinx.coroutines.flow.Flow

interface SubstrateSubscriptionBuilder {
    val socketService: SocketService?

    fun subscribe(key: String): Flow<StorageChange>
}

interface SharedRequestsBuilder : SubstrateSubscriptionBuilder
