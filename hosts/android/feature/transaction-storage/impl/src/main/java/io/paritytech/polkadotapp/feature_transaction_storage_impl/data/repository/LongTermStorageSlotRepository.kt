package io.paritytech.polkadotapp.feature_transaction_storage_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId

interface LongTermStorageSlotRepository {
    suspend fun periodDurationSeconds(chainId: ChainId): UInt

    suspend fun maxClaimsPerPeriod(): Result<UByte>

    suspend fun spentAliases(
        chainId: ChainId,
        period: UInt,
        candidates: List<BandersnatchAlias>,
    ): Set<BandersnatchAlias>
}
