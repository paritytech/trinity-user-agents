package io.paritytech.polkadotapp.feature_statement_store_impl.data.repository

import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.network.binding.BlockHash
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection

interface NotificationSlotRepository {
    /** Highest claimable seq for [collection]; seqs `0..=value` are valid. */
    suspend fun highestSeqPerPeriod(collection: PeopleCollection): Result<UByte>

    suspend fun registeredAliases(
        chainId: ChainId,
        candidates: Collection<BandersnatchAlias>,
    ): Result<Set<BandersnatchAlias>>

    /** Whether each of [accounts] holds any statement allowance at [at]; absent accounts were not answered. */
    suspend fun hasStatementAllowance(
        chainId: ChainId,
        accounts: Collection<AccountId>,
        at: BlockHash?,
    ): Result<Map<AccountId, Boolean>>
}
