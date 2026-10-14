package io.paritytech.polkadotapp.chains.util

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.domain.model.AccountId

interface AddressFormatter {
    suspend fun format(chainId: ChainId, accountId: AccountId): String
}

class RealAddressFormatter(
    private val chainRegistry: ChainRegistry
) : AddressFormatter {
    override suspend fun format(chainId: ChainId, accountId: AccountId): String {
        return chainRegistry.getChain(chainId).addressOf(accountId)
    }
}
