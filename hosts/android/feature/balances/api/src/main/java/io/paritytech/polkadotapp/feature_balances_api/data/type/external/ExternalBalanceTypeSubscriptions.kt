package io.paritytech.polkadotapp.feature_balances_api.data.type.external

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import kotlinx.coroutines.flow.Flow

interface ExternalBalanceTypeSubscriptions {
    suspend fun blockNumber(chainId: ChainId): Flow<BlockNumber>
}
