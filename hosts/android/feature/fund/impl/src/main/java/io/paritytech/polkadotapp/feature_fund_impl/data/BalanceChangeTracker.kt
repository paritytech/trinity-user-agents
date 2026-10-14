package io.paritytech.polkadotapp.feature_fund_impl.data

import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.requests.StorageSharedRequestsBuilderFactory
import io.paritytech.polkadotapp.chains.multiNetwork.requests.withSubscription
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.mergeIfMultiple
import io.paritytech.polkadotapp.common.utils.withFlowScope
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_balances_api.data.type.TokenBalanceTypeRegistry
import io.paritytech.polkadotapp.feature_balances_api.data.type.subscribeAccountBalance
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOn
import javax.inject.Inject

interface BalanceChangeTracker {
    fun balanceChanges(account: MetaAccount, assets: List<Chain.Asset>): Flow<TokenBalance>
}

internal class RealBalanceChangeTracker @Inject constructor(
    private val tokenBalanceTypeRegistry: TokenBalanceTypeRegistry,
    private val sharedRequestsBuilderFactory: StorageSharedRequestsBuilderFactory,
    private val coroutineDispatchers: CoroutineDispatchers,
    private val chainRegistry: ChainRegistry,
) : BalanceChangeTracker {
    override fun balanceChanges(
        account: MetaAccount,
        assets: List<Chain.Asset>
    ): Flow<TokenBalance> {
        return withFlowScope { scope ->
            with(scope) {
                assets.groupBy { it.chainId }
                    .map { (chainId, chainAssets) ->
                        val chain = chainRegistry.getChain(chainId)
                        val accountId = account.accountIdIn(chain)
                        balancesChangesForChain(accountId, chainId, chainAssets)
                    }
                    .mergeIfMultiple()
            }
        }
            .flowOn(coroutineDispatchers.io)
    }

    context(scope: CoroutineScope)
    private suspend fun balancesChangesForChain(
        accountId: AccountId,
        chainId: ChainId,
        assets: List<Chain.Asset>
    ): Flow<TokenBalance> {
        return sharedRequestsBuilderFactory.withSubscription(chainId) { builder ->
            assets.map { asset ->
                tokenBalanceTypeRegistry.typeFor(asset)
                    .subscribeAccountBalance(accountId, builder)
            }.mergeIfMultiple()
        }
    }
}
