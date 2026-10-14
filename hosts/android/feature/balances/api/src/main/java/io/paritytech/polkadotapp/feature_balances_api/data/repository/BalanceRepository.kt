package io.paritytech.polkadotapp.feature_balances_api.data.repository

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHold
import io.paritytech.polkadotapp.feature_balances_api.domain.model.BalanceHoldId
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

interface BalanceRepository {
    fun syncedTokenBalanceFlow(
        metaId: Long,
        chainAsset: Chain.Asset
    ): Flow<TokenBalance>

    suspend fun getSyncedTokenBalance(
        metaId: Long,
        chainAsset: Chain.Asset
    ): TokenBalance

    context(scope: ComputationalScope)
    fun observeBalanceHolds(chainId: ChainId, accountId: AccountId): Flow<List<BalanceHold>>
}

context(scope: ComputationalScope)
fun BalanceRepository.observeBalanceHoldById(
    chainId: ChainId,
    accountId: AccountId,
    balanceHoldId: BalanceHoldId
): Flow<BalanceHold?> {
    return observeBalanceHolds(chainId, accountId).map { holds ->
        holds.firstOrNull { it.id == balanceHoldId }
    }
}
